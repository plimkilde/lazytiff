use std::collections::BTreeMap;
use std::convert::{TryFrom, TryInto};
use std::io::{Read, Seek, BufReader};
use std::sync::{Arc, Mutex};

use crate::types::*;
use crate::types::FieldType::*;
use crate::error::TiffReadError;

#[derive(Debug)]
pub struct Subfile<R> {
    buf_reader_ref: Arc<Mutex<BufReader<R>>>,
    endianness: Endianness,
    pub fields: BTreeMap<u16, FieldState>,
    offset_to_next_ifd: Option<u32>,
}

impl<R: Read + Seek> Subfile<R> {
    pub fn new(buf_reader_ref: Arc<Mutex<BufReader<R>>>, offset: u32, endianness: Endianness) -> Result<Self, TiffReadError> {
        let ifd_entry_count: u16;
        let ifd_remaining_buffer_size: usize;
        let mut ifd_remaining_buffer: Vec<u8>;
        
        /* Restrict the borrow of buf_reader_ref to this scope so that
         * we can save it as a field in the output struct. */
        {
            let mut buf_reader = buf_reader_ref.lock().unwrap();
            
            buf_reader.seek(std::io::SeekFrom::Start(u64::from(offset)))?;
            
            let mut ifd_entry_count_bytes = [0u8; 2];
            buf_reader.read_exact(&mut ifd_entry_count_bytes)?;
            
            ifd_entry_count = match endianness {
                Endianness::Little => u16::from_le_bytes(ifd_entry_count_bytes),
                Endianness::Big => u16::from_be_bytes(ifd_entry_count_bytes),
            };
            
            // TODO: handle overflow
            ifd_remaining_buffer_size = 12*usize::from(ifd_entry_count) + 4;
            
            ifd_remaining_buffer = vec![0u8; ifd_remaining_buffer_size];
            
            /* Read remainder of the IFD now that we know how many bytes
             * to read. */
            buf_reader.read_exact(&mut ifd_remaining_buffer)?;
        }
        
        let mut fields_map = BTreeMap::new();
        for i in 0..usize::from(ifd_entry_count) {
            let ifd_entry_bytes: [u8; 12] = ifd_remaining_buffer[12*i..12*(i+1)].try_into().unwrap();
            
            let tag_bytes: [u8; 2] = ifd_entry_bytes[0..2].try_into().unwrap();
            let field_type_bytes: [u8; 2] = ifd_entry_bytes[2..4].try_into().unwrap();
            let num_values_bytes: [u8; 4] = ifd_entry_bytes[4..8].try_into().unwrap();
            let values_or_offset_bytes: [u8; 4] = ifd_entry_bytes[8..12].try_into().unwrap();
            
            let tag = match endianness {
                Endianness::Little => u16::from_le_bytes(tag_bytes),
                Endianness::Big => u16::from_be_bytes(tag_bytes),
            };
            
            let field_type = match endianness {
                Endianness::Little => u16::from_le_bytes(field_type_bytes),
                Endianness::Big => u16::from_be_bytes(field_type_bytes),
            };
            
            let num_values = match endianness {
                Endianness::Little => u32::from_le_bytes(num_values_bytes),
                Endianness::Big => u32::from_be_bytes(num_values_bytes),
            };
            
            let lazy_field_values = lazy_field_values_from_ifd_entry(field_type, num_values, values_or_offset_bytes, endianness);
            fields_map.insert(tag, lazy_field_values);
        }
        
        let ifd_offset_bytes: [u8; 4] = ifd_remaining_buffer[ifd_remaining_buffer_size-4..].try_into().unwrap();
        let raw_next_ifd_offset = match endianness {
            Endianness::Little => u32::from_le_bytes(ifd_offset_bytes),
            Endianness::Big => u32::from_be_bytes(ifd_offset_bytes),
        };
        
        let next_ifd_offset_opt = if raw_next_ifd_offset != 0 {
            Some(raw_next_ifd_offset)
        } else {
            None
        };
        
        Ok(Subfile {
            buf_reader_ref: buf_reader_ref,
            endianness: endianness,
            fields: fields_map,
            offset_to_next_ifd: next_ifd_offset_opt,
        })
    }
    
    pub fn offset_to_next_ifd(&self) -> Option<u32> {
        self.offset_to_next_ifd
    }
    
    fn get_field_buffer_size(field_type: FieldType, count: u32) -> Option<usize> {
        let element_size: usize = match field_type {
            Byte => 1,
            Ascii => 1,
            Short => 2,
            Long => 4,
            Rational => 8,
            SByte => 1,
            Undefined => 1,
            SShort => 2,
            SLong => 4,
            SRational => 8,
            Float => 4,
            Double => 8,
        };
        
        /* Return buffer size if `count` fits in a usize and the
         * multiplication doesn't overflow. */
        match usize::try_from(count) {
            Ok(count_usize) => element_size.checked_mul(count_usize),
            Err(_) => None,
        }
    }
}
