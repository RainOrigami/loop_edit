use anyhow::{Context, Result};
use libpulse_binding as pulse;
use libpulse_simple_binding as psimple;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

/// Audio playback for preview
pub struct AudioPlayback {
    samples: Arc<Vec<f32>>,
    sample_rate: u32,
    channels: u16,
    is_playing: Arc<AtomicBool>,
    playback_position: Arc<AtomicUsize>,
    _thread_handle: Option<std::thread::JoinHandle<()>>,
}

impl AudioPlayback {
    /// Create new playback instance
    pub fn new(samples: Vec<f32>, sample_rate: u32, channels: u16) -> Result<Self> {
        Ok(AudioPlayback {
            samples: Arc::new(samples),
            sample_rate,
            channels,
            is_playing: Arc::new(AtomicBool::new(false)),
            playback_position: Arc::new(AtomicUsize::new(0)),
            _thread_handle: None,
        })
    }

    /// Start playback
    pub fn play(&mut self) -> Result<()> {
        // Stop any existing playback
        self.stop();

        // Reset position
        self.playback_position.store(0, Ordering::Relaxed);
        self.is_playing.store(true, Ordering::Relaxed);

        let samples = self.samples.clone();
        let sample_rate = self.sample_rate;
        let channels = self.channels;
        let is_playing = self.is_playing.clone();
        let playback_position = self.playback_position.clone();

        // Spawn playback thread
        let thread_handle = std::thread::spawn(move || {
            if let Err(e) = run_playback(samples, sample_rate, channels, is_playing, playback_position) {
                log::error!("Playback error: {}", e);
            }
        });

        self._thread_handle = Some(thread_handle);
        Ok(())
    }

    /// Stop playback
    pub fn stop(&mut self) {
        self.is_playing.store(false, Ordering::Relaxed);
        if let Some(handle) = self._thread_handle.take() {
            let _ = handle.join();
        }
    }

    /// Check if currently playing
    pub fn is_playing(&self) -> bool {
        self.is_playing.load(Ordering::Relaxed)
    }

    /// Get current playback position (0.0 to 1.0)
    pub fn position(&self) -> f32 {
        let pos = self.playback_position.load(Ordering::Relaxed);
        let total = self.samples.len();
        if total > 0 {
            (pos as f32) / (total as f32)
        } else {
            0.0
        }
    }
}

fn run_playback(
    samples: Arc<Vec<f32>>,
    sample_rate: u32,
    channels: u16,
    is_playing: Arc<AtomicBool>,
    playback_position: Arc<AtomicUsize>,
) -> Result<()> {
    // Set up audio spec
    let spec = pulse::sample::Spec {
        format: pulse::sample::Format::F32le,
        channels: channels as u8,
        rate: sample_rate,
    };

    if !spec.is_valid() {
        anyhow::bail!("Invalid audio spec for playback");
    }

    // Create PulseAudio simple connection for playback
    let simple = psimple::Simple::new(
        None,                               // Use default server
        "loop_edit",                        // Application name
        pulse::stream::Direction::Playback, // We're playing back
        None,                               // Default playback device
        "Audio Preview",                    // Stream description
        &spec,                              // Sample format
        None,                               // No channel map
        None,                               // Default buffering attributes
    ).context("Failed to connect to playback device")?;

    // Playback loop
    let chunk_size = 1024 * channels as usize; // 1024 frames
    let mut position = 0;

    while is_playing.load(Ordering::Relaxed) && position < samples.len() {
        let remaining = samples.len() - position;
        let chunk_samples = remaining.min(chunk_size);
        
        let chunk = &samples[position..position + chunk_samples];
        
        // Convert to bytes
        let bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(
                chunk.as_ptr() as *const u8,
                chunk.len() * 4,
            )
        };

        // Write to playback
        simple.write(bytes).context("Failed to write audio data")?;
        
        position += chunk_samples;
        playback_position.store(position, Ordering::Relaxed);
    }

    // Drain remaining audio
    simple.drain().ok();
    
    is_playing.store(false, Ordering::Relaxed);
    Ok(())
}
