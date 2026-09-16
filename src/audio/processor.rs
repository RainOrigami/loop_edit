use parking_lot::RwLock;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use super::ring_buffer::AudioRingBuffer;

/// Waveform data point (downsampled)
#[derive(Clone, Debug)]
pub struct WaveformPoint {
    pub min: f32,
    pub max: f32
}

/// Cache of downsampled waveform data for rendering
#[derive(Clone, Debug)]
pub struct WaveformCache {
    pub points: Vec<WaveformPoint>,
    pub samples_per_point: usize,
}

impl WaveformCache {
    pub fn new() -> Self {
        Self {
            points: Vec::new(),
            samples_per_point: 1,
        }
    }

    /// Downsample audio buffer to target number of points
    /// Each point contains min/max for a window of samples
    /// Optimized for visual display - samples are skipped for performance
    pub fn from_buffer(buffer: &[f32], target_points: usize, channels: u16) -> Self {
        if buffer.is_empty() || target_points == 0 {
            return Self::new();
        }

        let channels = channels as usize;
        let total_frames = buffer.len() / channels;
        let samples_per_point = (total_frames / target_points).max(1);

        let mut points = Vec::with_capacity(target_points);

        // For visual display, we can skip samples for performance
        // Sample every 4th frame in each window - still gives accurate visual representation
        let frame_step = 4;

        for i in 0..target_points {
            let start_frame = i * samples_per_point;
            let end_frame = ((i + 1) * samples_per_point).min(total_frames);

            if start_frame >= total_frames {
                break;
            }

            // Extract window of samples - just use first channel for speed
            let mut min = f32::MAX;
            let mut max = f32::MIN;

            for frame in (start_frame..end_frame).step_by(frame_step) {
                let sample_idx = frame * channels;
                if sample_idx < buffer.len() {
                    let val = buffer[sample_idx];
                    min = min.min(val);
                    max = max.max(val);
                }
            }

            // Fallback if we didn't find any valid samples
            if min == f32::MAX {
                min = 0.0;
                max = 0.0;
            }

            points.push(WaveformPoint { min, max });
        }

        Self {
            points,
            samples_per_point,
        }
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }
}

/// Shared waveform cache that can be updated by processor and read by GUI
pub struct SharedWaveformCache {
    cache: Arc<RwLock<WaveformCache>>,
}

impl SharedWaveformCache {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(WaveformCache::new())),
        }
    }

    pub fn get(&self) -> Arc<RwLock<WaveformCache>> {
        self.cache.clone()
    }

    pub fn update(&self, new_cache: WaveformCache) {
        *self.cache.write() = new_cache;
    }
}

/// Start background thread that periodically updates waveform cache
pub fn start_waveform_processor(
    ring_buffer: Arc<RwLock<AudioRingBuffer>>,
    waveform_cache: Arc<RwLock<WaveformCache>>,
    target_points: usize,
) {
    thread::spawn(move || {
        log::info!("Waveform processor thread started (target: {} points)", target_points);
        
        // let mut last_write_pos = 0;

        loop {
            // Update every 100ms
            thread::sleep(Duration::from_millis(100));

            // Check if buffer has changed
            let (sample_rate, channels) = {
                let rb = ring_buffer.read();
                (rb.sample_rate(), rb.channels())
            };
            
            // // If write position hasn't moved much, skip update
            // let samples_written = if current_write_pos >= last_write_pos {
            //     current_write_pos - last_write_pos
            // } else {
            //     // Wrapped around
            //     let rb = ring_buffer.read();
            //     rb.capacity_samples() - last_write_pos + current_write_pos
            // };
            
            // // Only update if we've written at least 0.5 seconds of audio (avoids constant updates when idle)
            // let min_samples = (sample_rate as usize * channels as usize) / 2;
            // if samples_written < min_samples {
            //     continue;
            // }
            
            // last_write_pos = current_write_pos;

            // Read buffer and downsample
            // For better performance, only read last 15 seconds instead of full 60s buffer
            let max_samples = sample_rate as usize * channels as usize * 15;
            let (buffer_snapshot, channels) = {
                let rb = ring_buffer.read();
                let samples_to_read = rb.capacity_samples().min(max_samples);
                (rb.read_latest(samples_to_read), rb.channels())
            };

            // Downsample to cache (optimized for visual display)
            let new_cache = WaveformCache::from_buffer(&buffer_snapshot, target_points, channels);

            // Update shared cache
            *waveform_cache.write() = new_cache;
        }
    });
}
