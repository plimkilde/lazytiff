#[macro_use]
extern crate nom;
use std::collections::HashMap;
use std::io::{Read, Seek, BufRead, BufReader};
use std::fmt;
use std::error::Error;

mod types;
mod parsers;

#[derive(Debug)]
pub struct TiffReader<R> {
    endianness: nom::Endianness,
    buf_reader: std::io::BufReader<R>,
    offset_to_first_ifd: u32,
    subfile_fields_vec: Vec<SubfileFields>
}

#[derive(Debug)]
struct SubfileFields {
    fields: HashMap<u16, types::LazyFieldValues>
}

impl<R: Read + Seek> TiffReader<R> {
    pub fn new(reader: R) -> Result<Self, Box<dyn Error>> {
        let mut buf_reader = BufReader::new(reader);
        let mut header_bytes = [0u8; 8];
        buf_reader.seek(std::io::SeekFrom::Start(0))?;
        buf_reader.read_exact(&mut header_bytes)?;
        let header_parse_result = parsers::header(&header_bytes);
        match header_parse_result {
            Ok((_, header)) => {
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
                    Err(Box::new(TiffReadError))
                }
            }
            Err(_) => {
                Err(Box::new(TiffReadError))
            }
        }
    }
    
    pub fn read_all_ifds(&mut self) -> Result<(), Box<dyn Error>> {
        let mut ifd_offset = self.offset_to_first_ifd;
        while ifd_offset != 0 {
            self.buf_reader.seek(std::io::SeekFrom::Start(u64::from(ifd_offset)))?;
            
            let mut ifd_entry_count_buffer = [0u8; 2];
            self.buf_reader.read_exact(&mut ifd_entry_count_buffer)?;
            let ifd_entry_count_parse_result = nom::u16!(&ifd_entry_count_buffer, self.endianness);
            
            match ifd_entry_count_parse_result {
                Ok((_, ifd_entry_count)) => {
                    println!("ifd_entry_count: {}", ifd_entry_count);
                    let mut ifd_buffer: Vec<u8> = vec![0u8; 2 + 12*usize::from(ifd_entry_count) + 4];
                    self.buf_reader.seek(std::io::SeekFrom::Start(u64::from(ifd_offset)))?;
                    
                    self.buf_reader.read_exact(&mut ifd_buffer)?;
                    let ifd_parse_result = parsers::ifd(&ifd_buffer, self.endianness);
                    
                    match ifd_parse_result {
                        Ok((_, ifd)) => {
                            println!("Parsed IFD: {:?}", ifd);
                            let mut fields_map = HashMap::new();
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
                        Err(_) => {
                            return Err(Box::new(TiffReadError))
                        }
                    }
                }
                Err(_) => {
                    return Err(Box::new(TiffReadError))
                }
            }
        }
        
        Ok(())
    }
}

#[derive(Debug)]
struct TiffReadError;

impl fmt::Display for TiffReadError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "error reading TIFF file")
    }
}

impl Error for TiffReadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        // TODO
        None
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
        assert_eq!(tiff_reader.endianness, nom::Endianness::Little);
        assert_eq!(tiff_reader.offset_to_first_ifd, 1234567890u32);
        println!("{:#?}", tiff_reader);
    }
    
    #[test]
    fn create_tiff_reader_from_be_header() {
        let header_bytes = b"MM\x00\x2A\x49\x96\x02\xD2";
        let mut cursor = Cursor::new(header_bytes);
        let tiff_reader = crate::TiffReader::new(cursor).unwrap();
        assert_eq!(tiff_reader.endianness, nom::Endianness::Big);
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
        assert_eq!(tiff_reader.endianness, nom::Endianness::Little);
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
