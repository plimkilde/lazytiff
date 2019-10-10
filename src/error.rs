use std::fmt;

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
