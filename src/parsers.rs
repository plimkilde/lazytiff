use nom::{le_u32, le_i32};
use nom::{be_u32, be_i32};

use crate::types;
use crate::types::{FieldType, FieldValues, LazyFieldValues};

named!(pub header<types::Header>, do_parse!(
    endianness: alt!(
        value!(nom::Endianness::Little, tag!("II\x2A\x00")) |
        value!(nom::Endianness::Big, tag!("MM\x00\x2A"))
    ) >>
    offset_to_first_ifd: u32!(endianness) >>
    (types::Header {
        endianness: endianness,
        offset_to_first_ifd: offset_to_first_ifd
    })
));

named_args!(pub ifd(endianness: nom::Endianness)<types::Ifd>, do_parse!(
    num_directory_entries: u16!(endianness) >>
    directory_entries: count!(apply!(ifd_entry, endianness), usize::from(num_directory_entries)) >>
    offset_of_next_ifd: u32!(endianness) >>
    (types::Ifd {
        num_directory_entries: num_directory_entries,
        directory_entries: directory_entries,
        offset_of_next_ifd: offset_of_next_ifd
    })
));

named_args!(pub ifd_entry(endianness: nom::Endianness)<types::IfdEntry>, do_parse!(
    tag: u16!(endianness) >>
    field_type: u16!(endianness) >>
    num_values: u32!(endianness) >>
    values_or_offset: take!(4) >>
    (types::IfdEntry {
        tag: tag,
        field_type: field_type,
        num_values: num_values,
        values_or_offset: [values_or_offset[0], values_or_offset[1], values_or_offset[2], values_or_offset[3]]
    })
));

pub fn lazy_field_values_from_ifd_entry(ifd_entry: &types::IfdEntry, endianness: nom::Endianness) -> LazyFieldValues {
    // Used only if the values don't fit in the 4 bytes of the IFD entry.
    let offset = match endianness {
        nom::Endianness::Little => u32::from_le_bytes(ifd_entry.values_or_offset),
        nom::Endianness::Big => u32::from_be_bytes(ifd_entry.values_or_offset)
    };
    
    match ifd_entry.field_type {
        1 => { // BYTE
            if ifd_entry.num_values <= 4 {
                LazyFieldValues::Loaded(FieldValues::Byte(ifd_entry.values_or_offset[..ifd_entry.num_values as usize].to_vec()))
            }
            else {
                LazyFieldValues::NotLoaded {
                    field_type: FieldType::Byte,
                    num_values: ifd_entry.num_values,
                    offset: offset
                }
            }
        }
        2 => { // ASCII
            if ifd_entry.num_values <= 4 {
                LazyFieldValues::Loaded(FieldValues::Ascii(ifd_entry.values_or_offset[..ifd_entry.num_values as usize].to_vec()))
            }
            else {
                LazyFieldValues::NotLoaded {
                    field_type: FieldType::Ascii,
                    num_values: ifd_entry.num_values,
                    offset: offset
                }
            }
        }
        3 => { // SHORT
            if ifd_entry.num_values <= 2 {
                let mut values_vec: Vec<u16> = Vec::new();
                for i in 0..ifd_entry.num_values {
                    let value_bytes: [u8; 2] = [ifd_entry.values_or_offset[2*(i as usize)], ifd_entry.values_or_offset[2*(i as usize)+1]];
                    let value = match endianness {
                        nom::Endianness::Little => u16::from_le_bytes(value_bytes),
                        nom::Endianness::Big => u16::from_be_bytes(value_bytes)
                    };
                    values_vec.push(value);
                }
                LazyFieldValues::Loaded(FieldValues::Short(values_vec))
            }
            else
            {
                LazyFieldValues::NotLoaded {
                    field_type: FieldType::Short,
                    num_values: ifd_entry.num_values,
                    offset: offset
                }
            }
        }
        4 => { // LONG
            if ifd_entry.num_values <= 1 {
                let value = match endianness {
                    nom::Endianness::Little => u32::from_le_bytes(ifd_entry.values_or_offset),
                    nom::Endianness::Big => u32::from_be_bytes(ifd_entry.values_or_offset)
                };
                let values_vec = vec![value];
                LazyFieldValues::Loaded(FieldValues::Long(values_vec))
            }
            else
            {
                LazyFieldValues::NotLoaded {
                    field_type: FieldType::Long,
                    num_values: ifd_entry.num_values,
                    offset: offset
                }
            }
        }
        5 => { // RATIONAL
            LazyFieldValues::NotLoaded {
                field_type: FieldType::Rational,
                num_values: ifd_entry.num_values,
                offset: offset
            }
        }
        6 => { // SBYTE
            if ifd_entry.num_values <= 4 {
                let mut values_vec: Vec<i8> = Vec::new();
                for i in 0..ifd_entry.num_values as usize {
                    values_vec.push(ifd_entry.values_or_offset[i] as i8);
                }
                LazyFieldValues::Loaded(FieldValues::SByte(values_vec))
            }
            else {
                LazyFieldValues::NotLoaded {
                    field_type: FieldType::SByte,
                    num_values: ifd_entry.num_values,
                    offset: offset
                }
            }
        }
        7 => { // UNDEFINED
            if ifd_entry.num_values <= 4 {
                LazyFieldValues::Loaded(FieldValues::Undefined(ifd_entry.values_or_offset[..ifd_entry.num_values as usize].to_vec()))
            }
            else {
                LazyFieldValues::NotLoaded {
                    field_type: FieldType::Undefined,
                    num_values: ifd_entry.num_values,
                    offset: offset
                }
            }
        }
        8 => { // SSHORT
            if ifd_entry.num_values <= 2 {
                let mut values_vec: Vec<i16> = Vec::new();
                for i in 0..ifd_entry.num_values {
                    let value_bytes: [u8; 2] = [ifd_entry.values_or_offset[2*(i as usize)], ifd_entry.values_or_offset[2*(i as usize)+1]];
                    let value = match endianness {
                        nom::Endianness::Little => i16::from_le_bytes(value_bytes),
                        nom::Endianness::Big => i16::from_be_bytes(value_bytes)
                    };
                    values_vec.push(value);
                }
                LazyFieldValues::Loaded(FieldValues::SShort(values_vec))
            }
            else
            {
                LazyFieldValues::NotLoaded {
                    field_type: FieldType::SShort,
                    num_values: ifd_entry.num_values,
                    offset: offset
                }
            }
        }
        9 => { // SLONG
            if ifd_entry.num_values <= 1 {
                let value = match endianness {
                    nom::Endianness::Little => i32::from_le_bytes(ifd_entry.values_or_offset),
                    nom::Endianness::Big => i32::from_be_bytes(ifd_entry.values_or_offset)
                };
                let values_vec = vec![value];
                LazyFieldValues::Loaded(FieldValues::SLong(values_vec))
            }
            else
            {
                LazyFieldValues::NotLoaded {
                    field_type: FieldType::SLong,
                    num_values: ifd_entry.num_values,
                    offset: offset
                }
            }
        }
        10 => { // SRATIONAL
            LazyFieldValues::NotLoaded {
                field_type: FieldType::SRational,
                num_values: ifd_entry.num_values,
                offset: offset
            }
        }
        11 => { // FLOAT
            if ifd_entry.num_values <= 1 {
                let values_vec = match endianness {
                    nom::Endianness::Little => vec![f32::from_bits(u32::from_le_bytes(ifd_entry.values_or_offset))],
                    nom::Endianness::Big => vec![f32::from_bits(u32::from_be_bytes(ifd_entry.values_or_offset))]
                };
                LazyFieldValues::Loaded(FieldValues::Float(values_vec))
            }
            else {
                LazyFieldValues::NotLoaded {
                    field_type: FieldType::Float,
                    num_values: ifd_entry.num_values,
                    offset: offset
                }
            }
        }
        12 => { // DOUBLE
            LazyFieldValues::NotLoaded {
                field_type: FieldType::Double,
                num_values: ifd_entry.num_values,
                offset: offset
            }
        }
        _ => { // Type not specified in TIFF 6.0
            LazyFieldValues::Unknown {
                field_type: ifd_entry.field_type,
                num_values: ifd_entry.num_values,
                values_or_offset: ifd_entry.values_or_offset
            }
        }
    }
}

named_args!(pub rational(endianness: nom::Endianness)<types::Rational>, do_parse!(
    numerator: u32!(endianness) >>
    denominator: u32!(endianness) >>
    (types::Rational {
        numerator: numerator,
        denominator: denominator
    })
));

named!(pub le_rational<types::Rational>, do_parse!(
    numerator: le_u32 >>
    denominator: le_u32 >>
    (types::Rational {
        numerator: numerator,
        denominator: denominator
    })
));

named!(pub be_rational<types::Rational>, do_parse!(
    numerator: be_u32 >>
    denominator: be_u32 >>
    (types::Rational {
        numerator: numerator,
        denominator: denominator
    })
));

named_args!(pub srational(endianness: nom::Endianness)<types::SRational>, do_parse!(
    numerator: i32!(endianness) >>
    denominator: i32!(endianness) >>
    (types::SRational {
        numerator: numerator,
        denominator: denominator
    })
));

named!(pub le_srational<types::SRational>, do_parse!(
    numerator: le_i32 >>
    denominator: le_i32 >>
    (types::SRational {
        numerator: numerator,
        denominator: denominator
    })
));

named!(pub be_srational<types::SRational>, do_parse!(
    numerator: be_i32 >>
    denominator: be_i32 >>
    (types::SRational {
        numerator: numerator,
        denominator: denominator
    })
));
