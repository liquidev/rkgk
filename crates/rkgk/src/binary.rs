use std::{error::Error, fmt};

pub struct Reader<'a> {
    slice: &'a [u8],
}

impl<'a> Reader<'a> {
    pub fn new(slice: &'a [u8]) -> Self {
        Self { slice }
    }

    pub fn read_u8(&mut self) -> Result<u8, OutOfData> {
        if !self.slice.is_empty() {
            Ok(self.slice[0])
        } else {
            Err(OutOfData)
        }
    }

    pub fn read_u16(&mut self) -> Result<u16, OutOfData> {
        if self.slice.len() >= 2 {
            Ok(u16::from_le_bytes([self.slice[0], self.slice[1]]))
        } else {
            Err(OutOfData)
        }
    }

    pub fn read_u32(&mut self) -> Result<u32, OutOfData> {
        if self.slice.len() >= 4 {
            Ok(u32::from_le_bytes([
                self.slice[0],
                self.slice[1],
                self.slice[2],
                self.slice[3],
            ]))
        } else {
            Err(OutOfData)
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct OutOfData;

impl fmt::Display for OutOfData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("reader ran out of data")
    }
}

impl Error for OutOfData {}
