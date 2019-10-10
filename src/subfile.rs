use std::collections::BTreeMap;
use std::convert::TryInto;
use std::io::{Read, Seek, BufReader};
use std::sync::{Arc, Mutex};

use crate::types::*;
use crate::types::FieldType::*;
use crate::error::TiffReadError;
use crate::error::TiffReadError::*;

use FieldState::*;

#[derive(Debug, PartialEq, Clone)]
pub enum FieldState {
    //Loaded {FieldValues, offset_opt: Option<u32>}, // TODO
    Loaded(FieldValues),
    NotLoaded {field_type: FieldType, num_values: u32, offset: u32},
    Unknown {field_type: u16, num_values: u32, values_or_offset: [u8; 4]},
}

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
            
            let field_state = Self::get_lazy_field_state(field_type, num_values, values_or_offset_bytes, endianness)?;
            fields_map.insert(tag, field_state);
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
    
    // TODO: should be method of FieldState
    fn get_lazy_field_state(field_type_raw: u16, count: u32, values_or_offset: [u8; 4], endianness: Endianness) -> Result<FieldState, TiffReadError> {
        match type_from_u16(field_type_raw) {
            None => Ok(Unknown {field_type: field_type_raw, num_values: count, values_or_offset: values_or_offset}),
            Some(field_type) => {
                // TODO: new overflow error type?
                let required_buffer_size = compute_values_buffer_size(field_type, count).ok_or(ParseError)?;
                
                if required_buffer_size <= 4 {
                    /* The value(s) fit in the IFD entry, load them
                     * right away. */
                    let values_buffer = values_or_offset[..required_buffer_size].to_vec();
                    
                    let values = values_from_buffer(field_type, count, &values_buffer, endianness)?;
                    
                    Ok(Loaded(values))
                } else {
                    /* The value(s) did not fit in the IFD entry, skip
                     * loading data for now. */
                    let offset = match endianness {
                        Endianness::Little => u32::from_le_bytes(values_or_offset),
                        Endianness::Big => u32::from_be_bytes(values_or_offset),
                    };
                    
                    Ok(NotLoaded {field_type: field_type, num_values: count, offset: offset})
                }
            },
        }
    }
}
