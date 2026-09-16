use std::sync::Arc;

use crate::audio::{playback::AudioPlayback, WaveformCache};

/// Application state machine
pub enum AppState {
    /// Recording mode - ring buffer continuously recording
    Recording {
        live_waveform_cache: WaveformCache,
    },
    /// Editing mode - working on a snapshot of the buffer
    Editing {
        edit_buffer: Arc<Vec<f32>>,  // Snapshot from ring buffer
        sample_rate: u32,
        channels: u16,
        crop_start: f32,  // 0.0 to 1.0 (fraction of buffer)
        crop_end: f32,    // 0.0 to 1.0
        playback: Option<AudioPlayback>, // Playback instance
        waveform_cache: WaveformCache,
    },
}

impl AppState {
    pub fn initial() -> Self {
        AppState::Recording {
            live_waveform_cache: WaveformCache::new(),
        }
    }

    pub fn is_recording(&self) -> bool {
        matches!(self, AppState::Recording { .. })
    }

    pub fn is_editing(&self) -> bool {
        matches!(self, AppState::Editing { .. })
    }
}
