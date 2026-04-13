use thiserror::Error;

#[derive(Error, Debug)]
pub enum ChunkyError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid magic number: expected 0x{expected:08X}, got 0x{actual:08X}")]
    InvalidMagic { expected: u32, actual: u32 },

    #[error("Unsupported chunky version: {version} (minimum supported: {min})")]
    UnsupportedVersion { version: u16, min: u16 },

    #[error("Invalid byte order marker: 0x{0:04X}")]
    InvalidByteOrder(u16),

    #[error("Chunk not found: CTG=0x{ctg:08X} CNO={cno}")]
    ChunkNotFound { ctg: u32, cno: u32 },

    #[error("Invalid compression format: 0x{0:08X}")]
    InvalidCompressionFormat(u32),

    #[error("Decompression error: {0}")]
    DecompressionError(String),

    #[error("Unexpected end of data")]
    UnexpectedEof,

    #[error("Index corruption: {0}")]
    IndexCorruption(String),

    #[error("Invalid collection header: {0}")]
    InvalidCollection(String),
}

pub type Result<T> = std::result::Result<T, ChunkyError>;
