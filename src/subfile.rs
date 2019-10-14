use std::collections::BTreeMap;
use std::convert::TryInto;
use std::io::{Read, Seek, BufReader};
use std::sync::{Arc, Mutex};

use crate::types::*;
use crate::error::TiffReadError;
use crate::error::TiffReadError::*;

use FieldState::*;

#[derive(Debug, PartialEq, Clone)]
enum FieldState {
    Local(FieldValue),
    NotLoaded {field_type: FieldType, count: u32, offset: u32},
    Loaded {value: FieldValue, offset: u32},
    Unknown {field_type_raw: u16, count: u32, value_offset_bytes: [u8; 4]},
}

impl FieldState {
    fn from_ifd_entry_lazy(field_type_raw: u16, count: u32, value_offset_bytes: [u8; 4], endianness: Endianness) -> Result<FieldState, TiffReadError> {
        match FieldType::from_u16(field_type_raw) {
            None => Ok(Unknown {field_type_raw: field_type_raw, count: count, value_offset_bytes: value_offset_bytes}),
            Some(field_type) => {
                // TODO: new overflow error type?
                let required_buffer_size = compute_value_buffer_size(field_type, count).ok_or(ParseError)?;
                
                if required_buffer_size <= 4 {
                    /* The value(s) fit in the IFD entry, load them
                     * right away. */
                    let value_buffer = value_offset_bytes[..required_buffer_size].to_vec();
                    
                    let value = value_from_buffer(field_type, count, &value_buffer, endianness)?;
                    
                    Ok(Local(value))
                } else {
                    /* The value(s) did not fit in the IFD entry, skip
                     * loading data for now. */
                    let offset = match endianness {
                        Endianness::Little => u32::from_le_bytes(value_offset_bytes),
                        Endianness::Big => u32::from_be_bytes(value_offset_bytes),
                    };
                    
                    Ok(NotLoaded {field_type: field_type, count: count, offset: offset})
                }
            },
        }
    }
}

#[derive(Debug)]
pub struct Subfile<R> {
    buf_reader_ref: Arc<Mutex<BufReader<R>>>,
    endianness: Endianness,
    fields: BTreeMap<u16, FieldState>,
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
            let count_bytes: [u8; 4] = ifd_entry_bytes[4..8].try_into().unwrap();
            let value_offset_bytes: [u8; 4] = ifd_entry_bytes[8..12].try_into().unwrap();
            
            let tag: u16;
            let field_type_raw: u16;
            let count: u32;
            
            match endianness {
                Endianness::Little => {
                    tag = u16::from_le_bytes(tag_bytes);
                    field_type_raw = u16::from_le_bytes(field_type_bytes);
                    count = u32::from_le_bytes(count_bytes);
                }
                Endianness::Big => {
                    tag = u16::from_be_bytes(tag_bytes);
                    field_type_raw = u16::from_be_bytes(field_type_bytes);
                    count = u32::from_be_bytes(count_bytes);
                }
            }
            
            let field_state = FieldState::from_ifd_entry_lazy(field_type_raw, count, value_offset_bytes, endianness)?;
            fields_map.insert(tag, field_state);
        }
        
        let ifd_offset_bytes: [u8; 4] = ifd_remaining_buffer[ifd_remaining_buffer_size-4..].try_into().unwrap();
        let next_ifd_offset_raw = match endianness {
            Endianness::Little => u32::from_le_bytes(ifd_offset_bytes),
            Endianness::Big => u32::from_be_bytes(ifd_offset_bytes),
        };
        
        let next_ifd_offset_opt = if next_ifd_offset_raw != 0 {
            Some(next_ifd_offset_raw)
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
    
    /// Returns a `FieldValue` reference if the field value fit into
    /// the 4 bytes in the IFD. Will not trigger I/O operations.
    pub fn get_field_value_if_local(&self, tag: u16) -> Option<&FieldValue> {
        match self.fields.get(&tag) {
            Some(field_state) => {
                match field_state {
                    FieldState::Local(value) => Some(value),
                    _ => None,
                }
            }
            None => None,
        }
    }
    
    pub fn get_field_value(&mut self, tag: u16) -> Result<Option<&FieldValue>, TiffReadError> {
        // No-op if field is local
        self.load_field_value(tag)?;
        
        match self.fields.get(&tag) {
            Some(field_state) => {
                match field_state {
                    FieldState::Local(value) => Ok(Some(value)),
                    FieldState::Loaded {value, offset: _} => Ok(Some(value)),
                    _ => Ok(None),
                }
            }
            None => Ok(None),
        }
    }
    
    pub fn load_field_value(&mut self, tag: u16) -> Result<(), TiffReadError> {
        match self.fields.get(&tag) {
            Some(field_state) => {
                match *field_state {
                    FieldState::NotLoaded {field_type, count, offset} => {
                        // TODO: overflow error type
                        let required_buffer_size = compute_value_buffer_size(field_type, count).ok_or(ParseError)?;
                        let mut value_buffer = vec![0u8; required_buffer_size];
                        
                        let mut buf_reader = self.buf_reader_ref.lock().unwrap();
                        buf_reader.seek(std::io::SeekFrom::Start(u64::from(offset)))?;
                        buf_reader.read_exact(&mut value_buffer)?;
                        
                        let value = value_from_buffer(field_type.clone(), count, &value_buffer, self.endianness)?;
                        
                        self.fields.insert(tag, Loaded {value, offset});
                        
                        Ok(())
                    }
                    _ => Ok(()),
                }
            }
            None => Ok(()),
        }
    }
    
    pub fn unload_field_value(&mut self, tag: u16) {
        match self.fields.get_mut(&tag) {
            Some(field_state) => {
                match field_state {
                    FieldState::Loaded {value, offset} => {
                        let field_type = value.field_type();
                        let count_usize = value.count();
                        
                        /* The FieldValue will always be built from a
                         * u32 `count`, so this will always succeed. */
                        let count: u32 = count_usize.try_into().unwrap();
                        
                        /* needed to satisfy the borrow checker when
                         * inserting new FieldState in self.fields */
                        let offset: u32 = *offset;
                        
                        self.fields.insert(tag, NotLoaded {field_type: field_type, count: count, offset: offset});
                    }
                    _ => {},
                }
            }
            None => {},
        }
    }
}
