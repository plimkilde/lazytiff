#[derive(Debug)]
pub struct Header {
    pub endianness: nom::Endianness,
    pub offset_to_first_ifd: u32
}

#[derive(Debug)]
pub struct Ifd {
    pub num_directory_entries: u16,
    pub directory_entries: Vec<IfdEntry>,
    pub offset_of_next_ifd: u32
}

#[derive(Debug)]
pub struct IfdEntry {
    pub tag: u16,
    pub field_type: u16,
    pub num_values: u32,
    pub values_or_offset: [u8; 4]
}

#[derive(Debug, PartialEq)]
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

#[derive(Debug, PartialEq)]
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
    Double(Vec<f64>)           // 12
}

#[derive(Debug, PartialEq)]
pub enum LazyFieldValues {
    Loaded(FieldValues),
    NotLoaded {field_type: FieldType, num_values: u32, offset: u32},
    Unknown {field_type: u16, num_values: u32, values_or_offset: [u8; 4]}
}

#[derive(Debug, PartialEq)]
pub struct Rational {
    pub numerator: u32,
    pub denominator: u32
}

#[derive(Debug, PartialEq)]
pub struct SRational {
    pub numerator: i32,
    pub denominator: i32
}
