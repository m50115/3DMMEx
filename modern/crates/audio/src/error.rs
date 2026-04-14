//! Audio crate errors.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("WAV decode error: {0}")]
    WavDecode(String),

    #[error("MIDI parse error: {0}")]
    MidiParse(String),

    #[error("Audio playback unavailable: {0}")]
    PlaybackUnavailable(String),
}
