#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Endianness {
    Little,
    Big,
}

#[derive(Debug, PartialEq, Clone)]
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

#[derive(Debug, PartialEq, Clone)]
pub enum FieldValues {
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
pub enum FieldState {
    //Loaded {FieldValues, offset_opt: Option<u32>}, // TODO
    Loaded(FieldValues),
    NotLoaded {field_type: FieldType, num_values: u32, offset: u32},
    Unknown {field_type: u16, num_values: u32, values_or_offset: [u8; 4]},
}

#[derive(Debug, PartialEq, Clone)]
pub struct Rational {
    pub numerator: u32,
    pub denominator: u32
}

#[derive(Debug, PartialEq, Clone)]
pub struct SRational {
    pub numerator: i32,
    pub denominator: i32
}

pub fn lazy_field_values_from_ifd_entry(field_type: u16, num_values: u32, values_or_offset: [u8; 4], endianness: Endianness) -> FieldState {
    // Used only if the values don't fit in the 4 bytes of the IFD entry.
    let offset = match endianness {
        Endianness::Little => u32::from_le_bytes(values_or_offset),
        Endianness::Big => u32::from_be_bytes(values_or_offset)
    };
    
    match field_type {
        1 => { // BYTE
            if num_values <= 4 {
                FieldState::Loaded(FieldValues::Byte(values_or_offset[..num_values as usize].to_vec()))
            }
            else {
                FieldState::NotLoaded {
                    field_type: FieldType::Byte,
                    num_values: num_values,
                    offset: offset
                }
            }
        }
        2 => { // ASCII
            if num_values <= 4 {
                FieldState::Loaded(FieldValues::Ascii(values_or_offset[..num_values as usize].to_vec()))
            }
            else {
                FieldState::NotLoaded {
                    field_type: FieldType::Ascii,
                    num_values: num_values,
                    offset: offset
                }
            }
        }
        3 => { // SHORT
            if num_values <= 2 {
                let mut values_vec: Vec<u16> = Vec::new();
                for i in 0..num_values {
                    let value_bytes: [u8; 2] = [values_or_offset[2*(i as usize)], values_or_offset[2*(i as usize)+1]];
                    let value = match endianness {
                        Endianness::Little => u16::from_le_bytes(value_bytes),
                        Endianness::Big => u16::from_be_bytes(value_bytes)
                    };
                    values_vec.push(value);
                }
                FieldState::Loaded(FieldValues::Short(values_vec))
            }
            else
            {
                FieldState::NotLoaded {
                    field_type: FieldType::Short,
                    num_values: num_values,
                    offset: offset
                }
            }
        }
        4 => { // LONG
            if num_values <= 1 {
                let value = match endianness {
                    Endianness::Little => u32::from_le_bytes(values_or_offset),
                    Endianness::Big => u32::from_be_bytes(values_or_offset)
                };
                let values_vec = vec![value];
                FieldState::Loaded(FieldValues::Long(values_vec))
            }
            else
            {
                FieldState::NotLoaded {
                    field_type: FieldType::Long,
                    num_values: num_values,
                    offset: offset
                }
            }
        }
        5 => { // RATIONAL
            FieldState::NotLoaded {
                field_type: FieldType::Rational,
                num_values: num_values,
                offset: offset
            }
        }
        6 => { // SBYTE
            if num_values <= 4 {
                let mut values_vec: Vec<i8> = Vec::new();
                for i in 0..num_values as usize {
                    values_vec.push(values_or_offset[i] as i8);
                }
                FieldState::Loaded(FieldValues::SByte(values_vec))
            }
            else {
                FieldState::NotLoaded {
                    field_type: FieldType::SByte,
                    num_values: num_values,
                    offset: offset
                }
            }
        }
        7 => { // UNDEFINED
            if num_values <= 4 {
                FieldState::Loaded(FieldValues::Undefined(values_or_offset[..num_values as usize].to_vec()))
            }
            else {
                FieldState::NotLoaded {
                    field_type: FieldType::Undefined,
                    num_values: num_values,
                    offset: offset
                }
            }
        }
        8 => { // SSHORT
            if num_values <= 2 {
                let mut values_vec: Vec<i16> = Vec::new();
                for i in 0..num_values {
                    let value_bytes: [u8; 2] = [values_or_offset[2*(i as usize)], values_or_offset[2*(i as usize)+1]];
                    let value = match endianness {
                        Endianness::Little => i16::from_le_bytes(value_bytes),
                        Endianness::Big => i16::from_be_bytes(value_bytes)
                    };
                    values_vec.push(value);
                }
                FieldState::Loaded(FieldValues::SShort(values_vec))
            }
            else
            {
                FieldState::NotLoaded {
                    field_type: FieldType::SShort,
                    num_values: num_values,
                    offset: offset
                }
            }
        }
        9 => { // SLONG
            if num_values <= 1 {
                let value = match endianness {
                    Endianness::Little => i32::from_le_bytes(values_or_offset),
                    Endianness::Big => i32::from_be_bytes(values_or_offset)
                };
                let values_vec = vec![value];
                FieldState::Loaded(FieldValues::SLong(values_vec))
            }
            else
            {
                FieldState::NotLoaded {
                    field_type: FieldType::SLong,
                    num_values: num_values,
                    offset: offset
                }
            }
        }
        10 => { // SRATIONAL
            FieldState::NotLoaded {
                field_type: FieldType::SRational,
                num_values: num_values,
                offset: offset
            }
        }
        11 => { // FLOAT
            if num_values <= 1 {
                let values_vec = match endianness {
                    Endianness::Little => vec![f32::from_bits(u32::from_le_bytes(values_or_offset))],
                    Endianness::Big => vec![f32::from_bits(u32::from_be_bytes(values_or_offset))]
                };
                FieldState::Loaded(FieldValues::Float(values_vec))
            }
            else {
                FieldState::NotLoaded {
                    field_type: FieldType::Float,
                    num_values: num_values,
                    offset: offset
                }
            }
        }
        12 => { // DOUBLE
            FieldState::NotLoaded {
                field_type: FieldType::Double,
                num_values: num_values,
                offset: offset
            }
        }
        _ => { // Type not specified in TIFF 6.0
            FieldState::Unknown {
                field_type: field_type,
                num_values: num_values,
                values_or_offset: values_or_offset
            }
        }
    }
}
