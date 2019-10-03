use std::collections::BTreeMap;
use std::convert::TryInto;
use std::io::{Read, Seek, BufReader};
use std::fmt;
use std::sync::{Arc, Mutex};

use types::LazyFieldValues;

mod types;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Endianness {
    Little,
    Big,
}

#[derive(Debug)]
pub struct TiffReader<R> {
    endianness: Endianness,
    buf_reader_ref: Arc<Mutex<BufReader<R>>>,
    offset_to_first_ifd: u32,
    subfiles: Vec<Subfile<R>>,
}

#[derive(Debug)]
pub struct Header {
    pub endianness: Endianness,
    pub offset_to_first_ifd: u32
}

impl Header {
    fn from_bytes(bytes: &[u8; 8]) -> Result<Self, TiffReadError> {
        let endianness = match &bytes[0..4] {
            b"II\x2A\x00" => Endianness::Little,
            b"MM\x00\x2A" => Endianness::Big,
            _ => return Err(TiffReadError::ParseError)
        };
        
        let offset_bytes: [u8; 4] = bytes[4..8].try_into().unwrap();
        
        let offset_to_first_ifd = match endianness {
            Endianness::Little => u32::from_le_bytes(offset_bytes),
            Endianness::Big => u32::from_be_bytes(offset_bytes),
        };
        
        Ok(Header {
            endianness: endianness,
            offset_to_first_ifd: offset_to_first_ifd,
        })
    }
}

impl<R: Read + Seek> TiffReader<R> {
    pub fn new(reader: R) -> Result<Self, TiffReadError> {
        let mut buf_reader = BufReader::new(reader);
        let mut header_bytes = [0u8; 8];
        buf_reader.seek(std::io::SeekFrom::Start(0))?;
        buf_reader.read_exact(&mut header_bytes)?;
        let header = Header::from_bytes(&header_bytes)?;
        
        /* The TIFF 6.0 spec says at least one IFD is mandatory
         * (and that IFD needs to start after the header). */
        if header.offset_to_first_ifd >= 8 {
            Ok(TiffReader {
                endianness: header.endianness,
                buf_reader_ref: Arc::new(Mutex::new(buf_reader)),
                offset_to_first_ifd: header.offset_to_first_ifd,
                subfiles: Vec::new(),
            })
        }
        else {
            Err(TiffReadError::ParseError)
        }
    }
    
    pub fn read_all_ifds(&mut self) -> Result<(), TiffReadError> {
        let mut ifd_offset = self.offset_to_first_ifd;
        while ifd_offset != 0 {
            let subfile = Subfile::new(self.buf_reader_ref.clone(), ifd_offset, self.endianness)?;
            ifd_offset = subfile.offset_to_next_ifd().unwrap_or(0);
            self.subfiles.push(subfile);
        }
        
        Ok(())
    }
}

#[derive(Debug)]
pub struct Subfile<R> {
    buf_reader_ref: Arc<Mutex<BufReader<R>>>,
    endianness: Endianness,
    //fields: BTreeMap<u16, SubfileField>, // TODO
    fields: BTreeMap<u16, LazyFieldValues>,
    offset_to_next_ifd: Option<u32>,
}

impl<R: Read + Seek> Subfile<R> {
    fn new(buf_reader_ref: Arc<Mutex<BufReader<R>>>, offset: u32, endianness: Endianness) -> Result<Self, TiffReadError> {
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
            
            let lazy_field_values = types::lazy_field_values_from_ifd_entry(field_type, num_values, values_or_offset_bytes, endianness);
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
    
    fn offset_to_next_ifd(&self) -> Option<u32> {
        self.offset_to_next_ifd
    }
}

// TODO
/*#[derive(Debug)]
pub struct SubfileField {
    raw_field_type: u16,
    value_count: u32,
    raw_values_or_offset: [u8; 4],
    values_opt: LazyFieldValues,
}*/

#[derive(Debug)]
pub enum TiffReadError {
    IoError(std::io::Error),
    ParseError, // TODO: add payload
}

impl fmt::Display for TiffReadError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "error reading TIFF file")
    }
}

impl From<std::io::Error> for TiffReadError {
    fn from(error: std::io::Error) -> Self {
        TiffReadError::IoError(error)
    }
}

#[cfg(test)]
mod tests {
    use crate::types;
    use crate::Endianness;
    use std::io::Cursor;
    
    #[test]
    fn create_tiff_reader_from_le_header() {
        let header_bytes = b"II\x2A\x00\xD2\x02\x96\x49";
        let mut cursor = Cursor::new(header_bytes);
        let tiff_reader = crate::TiffReader::new(cursor).unwrap();
        assert_eq!(tiff_reader.endianness, Endianness::Little);
        assert_eq!(tiff_reader.offset_to_first_ifd, 1234567890u32);
        println!("{:#?}", tiff_reader);
    }
    
    #[test]
    fn create_tiff_reader_from_be_header() {
        let header_bytes = b"MM\x00\x2A\x49\x96\x02\xD2";
        let mut cursor = Cursor::new(header_bytes);
        let tiff_reader = crate::TiffReader::new(cursor).unwrap();
        assert_eq!(tiff_reader.endianness, Endianness::Big);
        assert_eq!(tiff_reader.offset_to_first_ifd, 1234567890u32);
        println!("{:#?}", tiff_reader);
    }
    
    #[test]
    #[should_panic]
    fn fail_create_tiff_reader_with_first_offset_too_low() {
        let header_bytes = b"II\x2A\x00\x00\x00\x00\x00";
        let mut cursor = Cursor::new(header_bytes);
        let tiff_reader = crate::TiffReader::new(cursor).unwrap();
        println!("{:#?}", tiff_reader); //should not be reachable
    }
    
    #[test]
    #[should_panic]
    fn fail_create_tiff_reader_from_incomplete_header() {
        let header_bytes = b"II\x2A\x00";
        let mut cursor = Cursor::new(header_bytes);
        let tiff_reader = crate::TiffReader::new(cursor).unwrap();
        println!("{:#?}", tiff_reader); //should not be reachable
    }
    
    #[test]
    #[should_panic]
    fn fail_create_tiff_reader_from_invalid_data() {
        let header_bytes = b"Hello, World!";
        let mut cursor = Cursor::new(header_bytes);
        let tiff_reader = crate::TiffReader::new(cursor).unwrap();
        println!("{:#?}", tiff_reader); //should not be reachable
    }
    
    #[test]
    fn read_ifd() {
        let tiff_bytes = [
            b"II\x2A\x00\x0D\x00\x00\x00".as_ref(), // image file header, offset 13 to first IFD
            b"\x00\x00\x00\x00\x00".as_ref(), // arbitrary spacing (5 bytes)
            b"\x01\x00".as_ref(), // IFD: number of entries (1)
            b"\x39\x05".as_ref(), // IFD entry: tag (1337)
            b"\x01\x00".as_ref(), // IFD entry: data type (1 = Byte)
            b"\x03\x00\x00\x00".as_ref(), // IFD entry: value count (3)
            b"\xCA\xFE\xBE\xEF".as_ref(), // IFD entry: values (3 bytes: 202, 254, 190)
            b"\x00\x00\x00\x00".as_ref() // IFD: offset to next IFD (0 = N/A)
        ].concat();
        let mut cursor = Cursor::new(tiff_bytes);
        let mut tiff_reader = crate::TiffReader::new(cursor).unwrap();
        println!("{:#?}", tiff_reader);
        assert_eq!(tiff_reader.endianness, Endianness::Little);
        assert_eq!(tiff_reader.offset_to_first_ifd, 13);
        tiff_reader.read_all_ifds().unwrap();
        assert_eq!(tiff_reader.subfiles.len(), 1);
        assert!(tiff_reader.subfiles[0].fields.contains_key(&1337));
        assert_eq!(
            tiff_reader.subfiles[0].fields.get(&1337).unwrap(),
            &types::LazyFieldValues::Loaded(types::FieldValues::Byte(vec![202, 254, 190]))
        );
        println!("{:#?}", tiff_reader);
    }
}
