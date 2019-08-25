#[macro_use]
extern crate nom;
use std::collections::BTreeMap;
use std::io::{Read, Seek, BufReader};
use std::fmt;

mod types;
mod parsers;

#[derive(Debug)]
pub struct TiffReader<R> {
    endianness: nom::number::Endianness,
    buf_reader: std::io::BufReader<R>,
    offset_to_first_ifd: u32,
    subfile_fields_vec: Vec<SubfileFields>
}

#[derive(Debug)]
struct SubfileFields {
    fields: BTreeMap<u16, types::LazyFieldValues>
}

impl<R: Read + Seek> TiffReader<R> {
    pub fn new(reader: R) -> Result<Self, TiffReadError> {
        let mut buf_reader = BufReader::new(reader);
        let mut header_bytes = [0u8; 8];
        buf_reader.seek(std::io::SeekFrom::Start(0))?;
        buf_reader.read_exact(&mut header_bytes)?;
        let header = parsers::header(&header_bytes)?.1;
        
        /* The TIFF 6.0 spec says at least one IFD is mandatory
         * (and that IFD needs to start after the header). */
        if header.offset_to_first_ifd >= 8 {
            Ok(TiffReader {
                endianness: header.endianness,
                buf_reader: buf_reader,
                offset_to_first_ifd: header.offset_to_first_ifd,
                subfile_fields_vec: Vec::new()
            })
        }
        else {
            Err(TiffReadError::ParseError)
        }
    }
    
    pub fn read_all_ifds(&mut self) -> Result<(), TiffReadError> {
        let mut ifd_offset = self.offset_to_first_ifd;
        while ifd_offset != 0 {
            self.buf_reader.seek(std::io::SeekFrom::Start(u64::from(ifd_offset)))?;
            
            let mut ifd_entry_count_buffer = [0u8; 2];
            self.buf_reader.read_exact(&mut ifd_entry_count_buffer)?;
            
            let ifd_entry_count = nom::u16!(&ifd_entry_count_buffer, self.endianness)?.1;
            
            let mut ifd_buffer: Vec<u8> = vec![0u8; 2 + 12*usize::from(ifd_entry_count) + 4];
            self.buf_reader.seek(std::io::SeekFrom::Start(u64::from(ifd_offset)))?;
            
            self.buf_reader.read_exact(&mut ifd_buffer)?;
            let ifd = parsers::ifd(&ifd_buffer, self.endianness)?.1;
            
            let mut fields_map = BTreeMap::new();
            for entry in ifd.directory_entries {
                let lazy_field_values = parsers::lazy_field_values_from_ifd_entry(&entry, self.endianness);
                
                fields_map.insert(entry.tag, lazy_field_values);
            }
            
            let subfile_fields = SubfileFields {
                fields: fields_map
            };
            self.subfile_fields_vec.push(subfile_fields);
            
            ifd_offset = ifd.offset_of_next_ifd;
        }
        
        Ok(())
    }
    
    fn get_subfile_lazy_field(&self, subfile: usize, tag: u16) -> Option<&types::LazyFieldValues> {
        self.subfile_fields_vec[subfile].fields.get(&tag)
    }
    
    fn get_subfile_field(&mut self, subfile: usize, tag: u16) -> Option<types::FieldValues> {
        let lazy_result = self.get_subfile_lazy_field(subfile, tag);
        match lazy_result {
            Some(types::LazyFieldValues::NotLoaded {field_type, num_values, offset}) => {
                let read_size = types::estimate_size(field_type, *num_values);
                let mut read_buffer: Vec<u8> = vec![0u8; read_size.unwrap() as usize]; //FIXME: "as" cast
                
                self.buf_reader.seek(std::io::SeekFrom::Start(0)).unwrap();
                self.buf_reader.read_exact(&mut read_buffer).unwrap();
                unreachable!() //PLACEHOLDER
            },
            Some(types::LazyFieldValues::Loaded(loaded_field_values)) => Some(loaded_field_values.clone()),
            Some(types::LazyFieldValues::Unknown {field_type, num_values, values_or_offset}) => None,
            None => None
        }
    }
}

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

impl From<nom::Err<(&[u8], nom::error::ErrorKind)>> for TiffReadError {
    fn from(error: nom::Err<(&[u8], nom::error::ErrorKind)>) -> Self {
        TiffReadError::ParseError
    }
}

#[cfg(test)]
mod tests {
    use crate::types;
    use std::io::Cursor;
    
    #[test]
    fn create_tiff_reader_from_le_header() {
        let header_bytes = b"II\x2A\x00\xD2\x02\x96\x49";
        let mut cursor = Cursor::new(header_bytes);
        let tiff_reader = crate::TiffReader::new(cursor).unwrap();
        assert_eq!(tiff_reader.endianness, nom::number::Endianness::Little);
        assert_eq!(tiff_reader.offset_to_first_ifd, 1234567890u32);
        println!("{:#?}", tiff_reader);
    }
    
    #[test]
    fn create_tiff_reader_from_be_header() {
        let header_bytes = b"MM\x00\x2A\x49\x96\x02\xD2";
        let mut cursor = Cursor::new(header_bytes);
        let tiff_reader = crate::TiffReader::new(cursor).unwrap();
        assert_eq!(tiff_reader.endianness, nom::number::Endianness::Big);
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
        assert_eq!(tiff_reader.endianness, nom::number::Endianness::Little);
        assert_eq!(tiff_reader.offset_to_first_ifd, 13);
        tiff_reader.read_all_ifds().unwrap();
        assert_eq!(tiff_reader.subfile_fields_vec.len(), 1);
        assert!(tiff_reader.subfile_fields_vec[0].fields.contains_key(&1337));
        assert_eq!(
            tiff_reader.subfile_fields_vec[0].fields.get(&1337).unwrap(),
            &types::LazyFieldValues::Loaded(types::FieldValues::Byte(vec![202, 254, 190]))
        );
        println!("{:#?}", tiff_reader);
    }
}
