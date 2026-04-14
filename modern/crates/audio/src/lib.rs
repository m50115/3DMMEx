//! # audio
//!
//! Audio engine: WAV playback and MIDI parsing for 3D Movie Maker.
//!
//! ## Modules
//!
//! - `wav`    — decode raw RIFF WAVE bytes → i16 PCM samples (`decode_wav`)
//! - `midi`   — parse Standard MIDI File bytes → event summary (`parse_midi`)
//! - `player` — rodio-based WAV playback (`AudioPlayer::try_new`)
//! - `error`  — `AudioError` type

pub mod error;
pub mod midi;
pub mod player;
pub mod wav;

pub use error::AudioError;
pub use midi::{MidiInfo, parse_midi};
pub use player::AudioPlayer;
pub use wav::{WavInfo, decode_wav};
