//! Reads a `SourceMap` back from bytes

use alloc::{string::String, vec::Vec};

use crate::source_map::{FunctionEntry, LabelEntry, MAGIC, ObjectEntry, SourceMap};

/// Errors that can occur during parsing
#[derive(Debug)]
pub enum ParseError {
    /// The image is too short to contain the expected data
    UnexpectedEof,
    /// The magic bytes do not match "QK3SMAP"
    InvalidMagic,
    /// A string in the image is not valid UTF-8
    InvalidUtf8,
}

impl core::fmt::Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ParseError::UnexpectedEof => write!(f, "unexpected end of image data"),
            ParseError::InvalidMagic => write!(
                f,
                "invalid magic bytes, expected {}",
                String::from_utf8(MAGIC.to_vec()).unwrap()
            ),
            ParseError::InvalidUtf8 => write!(f, "invalid utf-8 in image string data"),
        }
    }
}

/// Parse a `SourceMap` from raw bytes
///
/// # Errors
///
/// Returns a `ParseError` if the magic bytes are wrong, the data is
/// truncated, or any string is not valid UTF-8.
pub fn parse(bytes: &[u8]) -> Result<SourceMap, ParseError> {
    let mut r = Reader::new(bytes);

    // Expect the magic bytes at the start
    let magic = r.read_bytes(MAGIC.len())?;
    if magic != MAGIC {
        return Err(ParseError::InvalidMagic);
    }

    // Parse each component
    let functions = parse_functions(&mut r)?;
    let objects = parse_objects(&mut r)?;

    Ok(SourceMap { functions, objects })
}

fn parse_functions(r: &mut Reader) -> Result<Vec<FunctionEntry>, ParseError> {
    let count = r.read_u32()? as usize;

    // Parse each function entry
    let mut functions = Vec::with_capacity(count);
    for _ in 0..count {
        let index = r.read_u32()?;
        let name = r.read_string()?;

        // Parse label entries belonging to this function
        let labels = parse_labels(r)?;
        functions.push(FunctionEntry {
            index,
            name,
            labels,
        });
    }
    Ok(functions)
}

fn parse_labels(r: &mut Reader) -> Result<Vec<LabelEntry>, ParseError> {
    let count = r.read_u32()? as usize;

    // Parse each label
    let mut labels = Vec::with_capacity(count);
    for _ in 0..count {
        let offset = r.read_u32()?;
        let name = r.read_string()?;
        labels.push(LabelEntry { offset, name });
    }
    Ok(labels)
}

fn parse_objects(r: &mut Reader) -> Result<Vec<ObjectEntry>, ParseError> {
    let count = r.read_u32()? as usize;

    // Parse each object entry
    let mut objects = Vec::with_capacity(count);
    for _ in 0..count {
        let index = r.read_u32()?;
        let name = r.read_string()?;
        objects.push(ObjectEntry { index, name });
    }
    Ok(objects)
}

struct Reader<'a> {
    data: &'a [u8],
    cursor: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, cursor: 0 }
    }

    /// Returns the ramining number of bytes left in
    /// the data from the current cursor position
    fn remaining(&self) -> usize {
        self.data.len() - self.cursor
    }

    /// Reads a certain number of bytes from the reader, advancing
    /// the cursor by the number of bytes if successful
    ///
    /// # Errors
    ///
    /// If there is not the specified number of bytes left in the data,
    /// an `UnexpectedEof` error will be returned
    fn read_bytes(&mut self, count: usize) -> Result<&'a [u8], ParseError> {
        if self.remaining() < count {
            return Err(ParseError::UnexpectedEof);
        }
        let slice = &self.data[self.cursor..self.cursor + count];
        self.cursor += count;
        Ok(slice)
    }

    /// Expects to read a u32 from the data
    fn read_u32(&mut self) -> Result<u32, ParseError> {
        let bytes = self.read_bytes(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Expects to read a UTF-8 string from the data
    fn read_string(&mut self) -> Result<String, ParseError> {
        let len_bytes = self.read_bytes(2)?;
        let len = u16::from_le_bytes([len_bytes[0], len_bytes[1]]) as usize;
        let bytes = self.read_bytes(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| ParseError::InvalidUtf8)
    }
}
