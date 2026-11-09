use serde::Serialize;

pub const MAX_LINE: usize = 64 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub enum LineError {
    /// A line exceeded MAX_LINE; the connection must be closed.
    TooLong,
}

/// Incremental newline splitter with a hard size limit.
#[derive(Default)]
pub struct LineDecoder {
    buf: Vec<u8>,
}

impl LineDecoder {
    pub fn new() -> Self {
        Self::default()
    }
    /// Feed bytes; returns complete lines (without the newline), empty lines skipped.
    pub fn feed(&mut self, data: &[u8]) -> Result<Vec<Vec<u8>>, LineError> {
        let mut out = Vec::new();
        for &b in data {
            if b == b'\n' {
                if !self.buf.is_empty() {
                    out.push(std::mem::take(&mut self.buf));
                }
            } else {
                if self.buf.len() >= MAX_LINE {
                    self.buf.clear();
                    return Err(LineError::TooLong);
                }
                self.buf.push(b);
            }
        }
        Ok(out)
    }
}

/// Serialize a message as one JSON line.
pub fn encode_line<T: Serialize>(msg: &T) -> Vec<u8> {
    let mut v = serde_json::to_vec(msg).unwrap_or_else(|_| b"{}".to_vec());
    v.push(b'\n');
    v
}
