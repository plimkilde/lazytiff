use std::collections::BTreeMap;
use std::convert::TryInto;
use std::io::{Read, Seek, BufReader};
use std::sync::{Arc, Mutex};

use crate::types::*;
use crate::error::ParseError;

use FieldState::*;

#[derive(Debug, Clone)]
pub struct Field<R> {
    buf_reader_ref: Arc<Mutex<BufReader<R>>>,
    endianness: Endianness,
    state: FieldState,
}

impl<R: Read + Seek> Field<R> {
    pub fn field_type(&self) -> Option<FieldType> {
        match &self.state {
            FieldState::Local(value) => Some(value.field_type()),
            FieldState::NotLoaded {field_type, count: _, offset: _} => {
                Some(*field_type)
            }
            FieldState::Loaded {value, offset: _} => Some(value.field_type()),
            FieldState::Unknown {field_type_raw: _, count: _, value_offset_bytes: _} => None,
        }
    }
    
    pub fn count(&self) -> u32 {
        match &self.state {
            FieldState::Local(value) => {
                /* If we managed to build the FieldValue array in the
                 * first place, it did fit in a u32. */
                value.count().try_into().unwrap()
            }
            FieldState::NotLoaded {field_type: _, count, offset: _} => {
                *count
            }
            FieldState::Loaded {value, offset: _} => {
                value.count().try_into().unwrap()
            }
            FieldState::Unknown {field_type_raw: _, count, value_offset_bytes: _} => {
                *count
            }
        }
    }
    
    /// Returns a `FieldValue` reference if the field value fit into
    /// the 4 bytes in the IFD. Will not trigger I/O operations.
    pub fn get_value_if_local(&self) -> Option<&FieldValue> {
        match &self.state {
            FieldState::Local(value) => Some(&value),
            _ => None,
        }
    }
    
    pub fn get_value(&mut self) -> Result<Option<&FieldValue>, Box<dyn std::error::Error>> {
        self.load()?;
        
        match &self.state {
            FieldState::Local(value) => Ok(Some(&value)),
            FieldState::Loaded {value, offset: _} => Ok(Some(&value)),
            _ => Ok(None),
        }
    }
    
    pub fn load(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        match self.state {
            FieldState::NotLoaded {field_type, count, offset} => {
                // TODO: overflow error type
                let required_buffer_size = compute_value_buffer_size(field_type, count).ok_or(ParseError::new("Required buffer size too big".to_string()))?;
                let mut value_buffer = vec![0u8; required_buffer_size];
                
                let mut buf_reader = self.buf_reader_ref.lock().unwrap();
                buf_reader.seek(std::io::SeekFrom::Start(u64::from(offset)))?;
                buf_reader.read_exact(&mut value_buffer)?;
                
                let value = value_from_buffer(field_type.clone(), count, &value_buffer, self.endianness)?;
                
                self.state = FieldState::Loaded {value, offset};
                
                Ok(())
            }
            _ => Ok(()),
        }
    }
    
    pub fn unload(&mut self) {
        match &self.state {
            FieldState::Loaded {value, offset} => {
                let field_type = value.field_type();
                let count_usize = value.count();
                
                /* The FieldValue will always be built from a
                 * u32 `count`, so this will always succeed. */
                let count: u32 = count_usize.try_into().unwrap();
                
                let offset: u32 = *offset;
                
                self.state = FieldState::NotLoaded {field_type: field_type, count: count, offset: offset};
            }
            _ => {},
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
enum FieldState {
    Local(FieldValue),
    NotLoaded {field_type: FieldType, count: u32, offset: u32},
    Loaded {value: FieldValue, offset: u32},
    Unknown {field_type_raw: u16, count: u32, value_offset_bytes: [u8; 4]},
}

impl FieldState {
    fn from_ifd_entry_data(field_type_raw: u16, count: u32, value_offset_bytes: [u8; 4], endianness: Endianness) -> Result<FieldState, Box<dyn std::error::Error>> {
        match FieldType::from_u16(field_type_raw) {
            None => Ok(Unknown {field_type_raw: field_type_raw, count: count, value_offset_bytes: value_offset_bytes}),
            Some(field_type) => {
                // TODO: new overflow error type?
                let required_buffer_size = compute_value_buffer_size(field_type, count).ok_or(ParseError::new("Required buffer size too big".to_string()))?;
                
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
    fields: BTreeMap<u16, Field<R>>,
    offset_to_next_ifd: Option<u32>,
}

impl<R: Read + Seek> Subfile<R> {
    pub fn new(buf_reader_ref: Arc<Mutex<BufReader<R>>>, offset: u32, endianness: Endianness) -> Result<Self, Box<dyn std::error::Error>> {
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
            
            let field_state = FieldState::from_ifd_entry_data(field_type_raw, count, value_offset_bytes, endianness)?;
            let field = Field {
                buf_reader_ref: buf_reader_ref.clone(),
                endianness: endianness,
                state: field_state,
            };
            fields_map.insert(tag, field);
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
    
    pub fn get_field(&self, tag: u16) -> Option<&Field<R>> {
        self.fields.get(&tag)
    }
    
    pub fn get_field_mut(&mut self, tag: u16) -> Option<&mut Field<R>> {
        self.fields.get_mut(&tag)
    }
    
    pub fn load_all_field_values(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let tags: Vec<_> = self.fields.keys().cloned().collect();
        for tag in tags {
            self.get_field_mut(tag).unwrap().load()?;
        }
        Ok(())
    }
    
    pub fn unload_all_field_values(&mut self) {
        let tags: Vec<_> = self.fields.keys().cloned().collect();
        for tag in tags {
            self.get_field_mut(tag).unwrap().unload();
        }
    }
}
