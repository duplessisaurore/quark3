//! Writes a Quark3 `SourceMap` out to bytes for writing
//! to a file or elsewhere

use core::num::TryFromIntError;

use alloc::vec::Vec;

use crate::source_map::{FunctionEntry, LabelEntry, MAGIC, ObjectEntry, SourceMap};

/// Errors that can occur during the Writing out process
#[derive(Debug)]
pub enum WriteError {
    IntegerOverflow,
}

impl From<TryFromIntError> for WriteError {
    fn from(_: TryFromIntError) -> Self {
        Self::IntegerOverflow
    }
}

impl core::fmt::Display for WriteError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            WriteError::IntegerOverflow => {
                write!(f, "value is too large to encode into source map format")
            }
        }
    }
}

/// Write a `SourceMap` to raw bytes
///
/// # Errors
///
/// Returns a `WriteError` if any count or string length exceeds
/// the encoded format's bounds.
pub fn write(map: &SourceMap) -> Result<Vec<u8>, WriteError> {
    let mut w = Writer::new();

    w.write_bytes(MAGIC);
    write_functions(&mut w, &map.functions)?;
    write_objects(&mut w, &map.objects)?;

    Ok(w.finish())
}

/// Writes each function source map entry out
/// as a length-prefixed vec of entries
fn write_functions(w: &mut Writer, functions: &[FunctionEntry]) -> Result<(), WriteError> {
    w.try_write_len_u32(functions.len())?;
    for func in functions {
        w.write_u32(func.index);
        w.write_string(&func.name)?;
        write_labels(w, &func.labels)?;
    }
    Ok(())
}

/// Writes each label source map entry out
/// as a length-prefixed vec of entries
fn write_labels(w: &mut Writer, labels: &[LabelEntry]) -> Result<(), WriteError> {
    w.try_write_len_u32(labels.len())?;
    for label in labels {
        w.write_u32(label.offset);
        w.write_string(&label.name)?;
    }
    Ok(())
}

/// Writes each object source map entry out
/// as a length-prefixed vec of entries
fn write_objects(w: &mut Writer, objects: &[ObjectEntry]) -> Result<(), WriteError> {
    w.try_write_len_u32(objects.len())?;
    for obj in objects {
        w.write_u32(obj.index);
        w.write_string(&obj.name)?;
    }
    Ok(())
}

/// Internal cursor-based writer into a byte buffer
struct Writer {
    data: Vec<u8>,
}

impl Writer {
    fn new() -> Self {
        Self { data: Vec::new() }
    }

    /// Write some bytes into the data
    fn write_bytes(&mut self, bytes: &[u8]) {
        self.data.extend_from_slice(bytes);
    }

    /// Write some u32 into the data
    fn write_u32(&mut self, value: u32) {
        self.data.extend_from_slice(&value.to_le_bytes());
    }

    /// Write some string into the data as a u16 length
    /// prefixed bytes
    fn write_string(&mut self, value: &str) -> Result<(), WriteError> {
        let bytes = value.as_bytes();
        let len = u16::try_from(bytes.len())?;
        self.data.extend_from_slice(&len.to_le_bytes());
        self.data.extend_from_slice(bytes);
        Ok(())
    }

    /// Try write out some usize length as a u32
    ///
    /// # Errors
    ///
    /// Will return a `WriteError` if the usize does not fit
    /// in a u32.
    fn try_write_len_u32(&mut self, value: usize) -> Result<(), WriteError> {
        self.write_u32(u32::try_from(value)?);
        Ok(())
    }

    fn finish(self) -> Vec<u8> {
        self.data
    }
}
