extern crate num_rational;

use std::convert::TryInto;
use std::io::{Read, Seek, BufReader};
use std::sync::{Arc, Mutex};

use types::Endianness;
use subfile::Subfile;
use error::TiffReadError;

mod types;
mod subfile;
pub mod error;

#[derive(Debug)]
pub struct TiffReader<R> {
    endianness: Endianness,
    buf_reader_ref: Arc<Mutex<BufReader<R>>>,
    offset_to_first_ifd: u32,
    pub subfiles: Vec<Subfile<R>>,
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

#[cfg(test)]
mod tests {
    use crate::types;
    use crate::Endianness;
    use std::io::Cursor;
    
    #[test]
    fn create_tiff_reader_from_le_header() {
        let header_bytes = b"II\x2A\x00\xD2\x02\x96\x49";
        let cursor = Cursor::new(header_bytes);
        let tiff_reader = crate::TiffReader::new(cursor).unwrap();
        assert_eq!(tiff_reader.endianness, Endianness::Little);
        assert_eq!(tiff_reader.offset_to_first_ifd, 1234567890u32);
        println!("{:#?}", tiff_reader);
    }
    
    #[test]
    fn create_tiff_reader_from_be_header() {
        let header_bytes = b"MM\x00\x2A\x49\x96\x02\xD2";
        let cursor = Cursor::new(header_bytes);
        let tiff_reader = crate::TiffReader::new(cursor).unwrap();
        assert_eq!(tiff_reader.endianness, Endianness::Big);
        assert_eq!(tiff_reader.offset_to_first_ifd, 1234567890u32);
        println!("{:#?}", tiff_reader);
    }
    
    #[test]
    #[should_panic]
    fn fail_create_tiff_reader_with_first_offset_too_low() {
        let header_bytes = b"II\x2A\x00\x00\x00\x00\x00";
        let cursor = Cursor::new(header_bytes);
        let tiff_reader = crate::TiffReader::new(cursor).unwrap();
        println!("{:#?}", tiff_reader); //should not be reachable
    }
    
    #[test]
    #[should_panic]
    fn fail_create_tiff_reader_from_incomplete_header() {
        let header_bytes = b"II\x2A\x00";
        let cursor = Cursor::new(header_bytes);
        let tiff_reader = crate::TiffReader::new(cursor).unwrap();
        println!("{:#?}", tiff_reader); //should not be reachable
    }
    
    #[test]
    #[should_panic]
    fn fail_create_tiff_reader_from_invalid_data() {
        let header_bytes = b"Hello, World!";
        let cursor = Cursor::new(header_bytes);
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
        let cursor = Cursor::new(tiff_bytes);
        let mut tiff_reader = crate::TiffReader::new(cursor).unwrap();
        println!("{:#?}", tiff_reader);
        assert_eq!(tiff_reader.endianness, Endianness::Little);
        assert_eq!(tiff_reader.offset_to_first_ifd, 13);
        tiff_reader.read_all_ifds().unwrap();
        assert_eq!(tiff_reader.subfiles.len(), 1);
        assert_eq!(
            tiff_reader.subfiles[0].get_field_value_if_local(1337),
            Some(&types::FieldValue::Byte(vec![202, 254, 190]))
        );
        println!("{:#?}", tiff_reader);
    }
}
