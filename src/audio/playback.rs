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
    /// Samples written but not heard yet.
    latency: Arc<AtomicUsize>,
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
            latency: Arc::new(AtomicUsize::new(0)),
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
        let latency = self.latency.clone();
        self.latency.store(0, Ordering::Relaxed);

        // Spawn playback thread
        let thread_handle = std::thread::spawn(move || {
            let playing = is_playing.clone();
            if let Err(e) = run_playback(samples, sample_rate, channels, is_playing, playback_position, latency) {
                log::error!("Playback error: {}", e);
                playing.store(false, Ordering::Relaxed);
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

    /// Get current playback position (0.0 to 1.0): what is being heard,
    /// not what has been handed to the sound server.
    pub fn position(&self) -> f32 {
        let pos = self
            .playback_position
            .load(Ordering::Relaxed)
            .saturating_sub(self.latency.load(Ordering::Relaxed));
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
    latency: Arc<AtomicUsize>,
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
        if let Ok(us) = simple.get_latency() {
            let samples = us.0 as usize * sample_rate as usize / 1_000_000 * channels as usize;
            latency.store(samples, Ordering::Relaxed);
        }
        playback_position.store(position, Ordering::Relaxed);
    }

    if is_playing.load(Ordering::Relaxed) {
        // Let the end be heard, moving the playhead along while it is.
        let left = latency.load(Ordering::Relaxed);
        let started = std::time::Instant::now();
        let per_second = sample_rate as f64 * channels as f64;
        while is_playing.load(Ordering::Relaxed) {
            let gone = (started.elapsed().as_secs_f64() * per_second) as usize;
            latency.store(left.saturating_sub(gone), Ordering::Relaxed);
            if gone >= left {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        simple.drain().ok();
    } else {
        // Stopped: silence now, not after the queue runs out.
        simple.flush().ok();
    }
    
    is_playing.store(false, Ordering::Relaxed);
    Ok(())
}
