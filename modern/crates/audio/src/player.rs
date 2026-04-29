//! Audio playback via rodio.
//!
//! `AudioPlayer` wraps a rodio `Sink` for WAV playback.  Construction may fail
//! on headless systems (no audio device) — call `try_new()` and handle `None`
//! gracefully.

use std::io::Cursor;

use crate::error::AudioError;

// ── AudioPlayer ───────────────────────────────────────────────────────────────

/// rodio-based audio player.
///
/// Holds an `OutputStream` (must stay alive for playback) and a `Sink`.
/// Use `try_new()` to construct; returns `None` when no audio device is available.
pub struct AudioPlayer {
    // OutputStream must remain alive for the duration of playback.
    _stream: rodio::OutputStream,
    sink: rodio::Sink,
}

impl AudioPlayer {
    /// Try to open the default audio output device.
    ///
    /// Returns `None` if no audio device is available (e.g. headless CI).
    pub fn try_new() -> Option<Self> {
        let (stream, stream_handle) = rodio::OutputStream::try_default().ok()?;
        let sink = rodio::Sink::try_new(&stream_handle).ok()?;
        Some(Self {
            _stream: stream,
            sink,
        })
    }

    /// Enqueue raw RIFF WAV bytes for playback.
    ///
    /// Playback begins immediately if the sink is not paused.
    pub fn play_wav(&self, data: Vec<u8>) -> Result<(), AudioError> {
        let cursor = Cursor::new(data);
        let source = rodio::Decoder::new(cursor)
            .map_err(|e| AudioError::PlaybackUnavailable(e.to_string()))?;
        self.sink.append(source);
        Ok(())
    }

    /// Block until all queued audio has finished playing.
    pub fn wait_until_done(&self) {
        self.sink.sleep_until_end();
    }

    /// Stop all queued audio immediately.
    pub fn stop(&self) {
        self.sink.stop();
    }

    /// Pause playback.
    pub fn pause(&self) {
        self.sink.pause();
    }

    /// Resume paused playback.
    pub fn resume(&self) {
        self.sink.play();
    }

    /// True if the sink is currently paused.
    pub fn is_paused(&self) -> bool {
        self.sink.is_paused()
    }

    /// Set playback volume (0.0 = silent, 1.0 = full).
    pub fn set_volume(&self, volume: f32) {
        self.sink.set_volume(volume);
    }
}
