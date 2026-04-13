//! # chunky-format
//!
//! Bit-perfect reader/writer for Microsoft 3D Movie Maker chunky file format (CHN2).
//!
//! This crate implements the Chunky file format used by 3D Movie Maker (1995)
//! and its source port 3DMMEx. It provides:
//!
//! - Reading and writing CFL (Chunky File) containers
//! - KCDC/KCD2 decompression and compression
//! - Serialization of GL, GG, GST collections
//! - Byte-order handling (little-endian Windows / big-endian Mac)
//!
//! ## Compatibility Guarantee
//!
//! Files written by this crate MUST be readable by the original 1995 3D Movie Maker.
//! Round-trip (read → write) of unmodified files MUST produce byte-identical output.

pub mod error;
pub mod bom;
pub mod chunk;
pub mod cfl;
pub mod codec;
pub mod collections;
pub mod tag;

pub use error::{ChunkyError, Result};
pub use chunk::{ChunkId, ChunkEntry, ChildRef, ChunkFlags};
pub use cfl::ChunkyFile;
pub use tag::ResourceTag;
