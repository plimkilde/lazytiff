use std::fmt;

#[derive(Debug)]
pub struct ParseError {
    message: String,
}

impl ParseError {
    pub fn new(message: String) -> Self {
        ParseError {
            message: message,
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ParseError {
}

pub fn escaped_string_from_bytes(bytes: &[u8]) -> String {
    let escaped_bytes: Vec<u8> = bytes.iter().map(|c| std::ascii::escape_default(*c)).flatten().collect();
    String::from_utf8_lossy(&escaped_bytes).to_string()
}
