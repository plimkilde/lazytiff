use std::convert::{TryFrom, TryInto};
use std::slice::ChunksExact;

use crate::error::TiffReadError;

use FieldType::*;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Endianness {
    Little,
    Big,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum FieldType {
    Byte,      //  1
    Ascii,     //  2
    Short,     //  3
    Long,      //  4
    Rational,  //  5
    SByte,     //  6
    Undefined, //  7
    SShort,    //  8
    SLong,     //  9
    SRational, // 10
    Float,     // 11
    Double,    // 12
}

pub fn type_from_u16(field_type_raw: u16) -> Option<FieldType> {
    match field_type_raw {
        1 => Some(Byte),
        2 => Some(Ascii),
        3 => Some(Short),
        4 => Some(Long),
        5 => Some(Rational),
        6 => Some(SByte),
        7 => Some(Undefined),
        8 => Some(SShort),
        9 => Some(SLong),
        10 => Some(SRational),
        11 => Some(Float),
        12 => Some(Double),
        _ => None,
    }
}

pub fn size_of_type(field_type: FieldType) -> usize {
    match field_type {
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
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum FieldValue {
    Byte(Vec<u8>),             //  1
    Ascii(Vec<u8>),            //  2 TODO: when to convert to std::ffi::CStr??
    Short(Vec<u16>),           //  3
    Long(Vec<u32>),            //  4
    Rational(Vec<Rational>),   //  5
    SByte(Vec<i8>),            //  6
    Undefined(Vec<u8>),        //  7
    SShort(Vec<i16>),          //  8
    SLong(Vec<i32>),           //  9
    SRational(Vec<SRational>), // 10
    Float(Vec<f32>),           // 11
    Double(Vec<f64>),          // 12
}

#[derive(Debug, PartialEq, Clone)]
pub struct Rational {
    pub numerator: u32,
    pub denominator: u32,
}

#[derive(Debug, PartialEq, Clone)]
pub struct SRational {
    pub numerator: i32,
    pub denominator: i32,
}

impl Rational {
    fn from_le_bytes(bytes: [u8; 8]) -> Self {
        let numerator = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        let denominator = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        Rational {
            numerator: numerator,
            denominator: denominator,
        }
    }
    
    fn from_be_bytes(bytes: [u8; 8]) -> Self {
        let numerator = u32::from_be_bytes(bytes[0..4].try_into().unwrap());
        let denominator = u32::from_be_bytes(bytes[4..8].try_into().unwrap());
        Rational {
            numerator: numerator,
            denominator: denominator,
        }
    }
}

impl SRational {
    fn from_le_bytes(bytes: [u8; 8]) -> Self {
        let numerator = i32::from_le_bytes(bytes[0..4].try_into().unwrap());
        let denominator = i32::from_le_bytes(bytes[4..8].try_into().unwrap());
        SRational {
            numerator: numerator,
            denominator: denominator,
        }
    }
    
    fn from_be_bytes(bytes: [u8; 8]) -> Self {
        let numerator = i32::from_be_bytes(bytes[0..4].try_into().unwrap());
        let denominator = i32::from_be_bytes(bytes[4..8].try_into().unwrap());
        SRational {
            numerator: numerator,
            denominator: denominator,
        }
    }
}

pub fn compute_value_buffer_size(field_type: FieldType, count: u32) -> Option<usize> {
    let element_size = size_of_type(field_type);
    
    /* Return buffer size if `count` fits in a usize and the
     * multiplication doesn't overflow. */
    match usize::try_from(count) {
        Ok(count_usize) => element_size.checked_mul(count_usize),
        Err(_) => None,
    }
}

pub fn value_from_buffer(field_type: FieldType, count: u32, buffer: &[u8], endianness: Endianness) -> Result<FieldValue, TiffReadError> {
    let count_usize = match usize::try_from(count) {
        Ok(count_usize) => count_usize,
        Err(_) => return Err(TiffReadError::ParseError),
    };
    let type_size = size_of_type(field_type);
    let correct_buffer_size = compute_value_buffer_size(field_type, count).ok_or(TiffReadError::ParseError)?;
    
    if buffer.len() == correct_buffer_size {
        let buffer_chunks = buffer.chunks_exact(type_size);
        
        let value = value_from_chunks(field_type, buffer_chunks, endianness);
        
        Ok(value)
    } else {
        Err(TiffReadError::ParseError)
    }
}

fn value_from_chunks(field_type: FieldType, chunks: ChunksExact<u8>, endianness: Endianness) -> FieldValue {
    /* The BYTE, ASCII, SBYTE and UNDEFINED data types are not endian-
     * sensitive. */
    match field_type {
        Byte => FieldValue::Byte(chunks.map(|chunk| chunk[0]).collect()),
        Ascii => FieldValue::Ascii(chunks.map(|chunk| chunk[0]).collect()),
        Short => {
            let values_iter: Box<dyn Iterator<Item = u16>> = match endianness {
                Endianness::Little => Box::new(chunks.map(|chunk_bytes| u16::from_le_bytes(chunk_bytes.try_into().unwrap()))),
                Endianness::Big => Box::new(chunks.map(|chunk_bytes| u16::from_be_bytes(chunk_bytes.try_into().unwrap()))),
            };
            
            FieldValue::Short(values_iter.collect())
        }
        Long => {
            let values_iter: Box<dyn Iterator<Item = u32>> = match endianness {
                Endianness::Little => Box::new(chunks.map(|chunk_bytes| u32::from_le_bytes(chunk_bytes.try_into().unwrap()))),
                Endianness::Big => Box::new(chunks.map(|chunk_bytes| u32::from_be_bytes(chunk_bytes.try_into().unwrap()))),
            };
            
            FieldValue::Long(values_iter.collect())
        }
        Rational => {
            let values_iter: Box<dyn Iterator<Item = Rational>> = match endianness {
                Endianness::Little => Box::new(chunks.map(|chunk_bytes| Rational::from_le_bytes(chunk_bytes.try_into().unwrap()))),
                Endianness::Big => Box::new(chunks.map(|chunk_bytes| Rational::from_be_bytes(chunk_bytes.try_into().unwrap()))),
            };
            
            FieldValue::Rational(values_iter.collect())
        }
        SByte => FieldValue::SByte(chunks.map(|chunk| chunk[0] as i8).collect()),
        Undefined => FieldValue::Undefined(chunks.map(|chunk| chunk[0]).collect()),
        SShort => {
            let values_iter: Box<dyn Iterator<Item = i16>> = match endianness {
                Endianness::Little => Box::new(chunks.map(|chunk_bytes| i16::from_le_bytes(chunk_bytes.try_into().unwrap()))),
                Endianness::Big => Box::new(chunks.map(|chunk_bytes| i16::from_be_bytes(chunk_bytes.try_into().unwrap()))),
            };
            
            FieldValue::SShort(values_iter.collect())
        }
        SLong => {
            let values_iter: Box<dyn Iterator<Item = i32>> = match endianness {
                Endianness::Little => Box::new(chunks.map(|chunk_bytes| i32::from_le_bytes(chunk_bytes.try_into().unwrap()))),
                Endianness::Big => Box::new(chunks.map(|chunk_bytes| i32::from_be_bytes(chunk_bytes.try_into().unwrap()))),
            };
            
            FieldValue::SLong(values_iter.collect())
        }
        SRational => {
            let values_iter: Box<dyn Iterator<Item = SRational>> = match endianness {
                Endianness::Little => Box::new(chunks.map(|chunk_bytes| SRational::from_le_bytes(chunk_bytes.try_into().unwrap()))),
                Endianness::Big => Box::new(chunks.map(|chunk_bytes| SRational::from_be_bytes(chunk_bytes.try_into().unwrap()))),
            };
            
            FieldValue::SRational(values_iter.collect())
        }
        Float => {
            let values_iter: Box<dyn Iterator<Item = f32>> = match endianness {
                Endianness::Little => Box::new(chunks.map(|chunk_bytes| f32::from_bits(u32::from_le_bytes(chunk_bytes.try_into().unwrap())))),
                Endianness::Big => Box::new(chunks.map(|chunk_bytes| f32::from_bits(u32::from_be_bytes(chunk_bytes.try_into().unwrap())))),
            };
            
            FieldValue::Float(values_iter.collect())
        }
        Double => {
            let values_iter: Box<dyn Iterator<Item = f64>> = match endianness {
                Endianness::Little => Box::new(chunks.map(|chunk_bytes| f64::from_bits(u64::from_le_bytes(chunk_bytes.try_into().unwrap())))),
                Endianness::Big => Box::new(chunks.map(|chunk_bytes| f64::from_bits(u64::from_be_bytes(chunk_bytes.try_into().unwrap())))),
            };
            
            FieldValue::Double(values_iter.collect())
        }
    }
}
