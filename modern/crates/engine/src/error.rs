//! Engine-level errors.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Chunky format error: {0}")]
    Chunky(#[from] chunky_format::ChunkyError),

    #[error("Unexpected end of data reading {what}: need {need} bytes, got {got}")]
    UnexpectedEof { what: &'static str, need: usize, got: usize },

    #[error("Invalid byte order marker: {0:#06x}")]
    InvalidByteOrder(u16),

    #[error("Unsupported version: cur={cur}, back={back}")]
    UnsupportedVersion { cur: i16, back: i16 },

    #[error("Invalid tag: sid={sid} ctg={ctg:#010x} cno={cno}")]
    InvalidTag { sid: i32, ctg: u32, cno: u32 },

    #[error("Missing required chunk {ctg} / {cno} in scene {scene_idx}")]
    MissingChunk { ctg: String, cno: u32, scene_idx: usize },

    #[error("Actor event type unknown: {0}")]
    UnknownEventType(i32),

    #[error("Data out of range: {what} = {value}")]
    OutOfRange { what: &'static str, value: i64 },
}

pub type EngineResult<T> = Result<T, EngineError>;
