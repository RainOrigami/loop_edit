use anyhow::{Context, Result};
use libpulse_binding as pulse;
use libpulse_simple_binding as psimple;
use parking_lot::{Mutex, RwLock};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::ring_buffer::AudioRingBuffer;

/// PulseAudio audio capture (works with PipeWire via PA compatibility layer)
pub struct AudioCapture {
    thread_handle: Option<std::thread::JoinHandle<()>>,
    should_stop: Arc<AtomicBool>,
    error: Arc<Mutex<Option<String>>>,
}

impl AudioCapture {
    /// Create and start PulseAudio audio capture
    pub fn new(
        device_name: Option<&str>,
        sample_rate: u32,
        channels: u16,
        ring_buffer: Arc<RwLock<AudioRingBuffer>>,
    ) -> Result<Self> {
        let device_name = device_name.map(|s| s.to_string());

        log::info!("Starting PulseAudio audio capture thread");
        if let Some(ref dev) = device_name {
            log::info!("  Target device: '{}'", dev);
        } else {
            log::info!("  Using default device");
        }

        let should_stop = Arc::new(AtomicBool::new(false));
        let should_stop_clone = should_stop.clone();
        let error = Arc::new(Mutex::new(None));
        let error_clone = error.clone();

        // Spawn audio capture thread
        let thread_handle = std::thread::spawn(move || {
            if let Err(e) = run_capture_loop(device_name, sample_rate, channels, ring_buffer, should_stop_clone) {
                log::error!("Audio capture error: {}", e);
                *error_clone.lock() = Some(format!("{e:#}"));
            }
        });

        Ok(AudioCapture {
            thread_handle: Some(thread_handle),
            should_stop,
            error,
        })
    }

    /// Why capture stopped, if it did.
    pub fn error(&self) -> Option<String> {
        self.error.lock().clone()
    }

    /// List all available PulseAudio/PipeWire audio sources
    pub fn list_devices() -> Result<Vec<String>> {
        log::info!("Device listing via PulseAudio API");
        
        Ok(vec![
            "To list PulseAudio/PipeWire sources, run: pactl list sources short".to_string(),
            "For monitors (capture system audio): use 'device_name.monitor'".to_string(),
            "Example: 'media.monitor' to capture from 'media' sink".to_string(),
        ])
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        log::info!("Stopping audio capture...");
        self.should_stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.thread_handle.take() {
            // Give thread a moment to exit cleanly
            std::thread::sleep(std::time::Duration::from_millis(50));
            let _ = handle.join();
        }
        log::info!("Audio capture stopped");
    }
}

fn run_capture_loop(
    device_name: Option<String>,
    sample_rate: u32,
    channels: u16,
    ring_buffer: Arc<RwLock<AudioRingBuffer>>,
    should_stop: Arc<AtomicBool>,
) -> Result<()> {
    // Set up audio spec
    let spec = pulse::sample::Spec {
        format: pulse::sample::Format::F32le,
        channels: channels as u8,
        rate: sample_rate,
    };

    if !spec.is_valid() {
        anyhow::bail!("Invalid audio spec: {} channels, {}Hz", channels, sample_rate);
    }

    // Create PulseAudio simple connection
    let simple = if let Some(ref device) = device_name {
        psimple::Simple::new(
            None,                               // Use default server
            "loop_edit",                        // Application name
            pulse::stream::Direction::Record,   // We're recording
            Some(device.as_str()),              // Specific device
            "Audio Capture",                    // Stream description
            &spec,                              // Sample format
            None,                               // No channel map
            None,                               // Default buffering attributes
        ).context(format!("Failed to connect to device '{}'", device))?
    } else {
        psimple::Simple::new(
            None,
            "loop_edit",
            pulse::stream::Direction::Record,
            None,                               // Default device
            "Audio Capture",
            &spec,
            None,
            None,
        ).context("Failed to connect to default audio device")?
    };

    log::info!("PulseAudio audio capture started");
    log::info!("  Format: {} channels, {}Hz", channels, sample_rate);

    // Buffer for reading audio data - smaller size for more responsive shutdown
    let buffer_size = 512 * 4; // Bytes (128 stereo f32 samples)
    let mut buffer = vec![0u8; buffer_size];

    while !should_stop.load(Ordering::Relaxed) {
        // Read audio data from PulseAudio (this is a blocking call)
        match simple.read(&mut buffer) {
            Ok(()) => {
                // Convert bytes to f32 samples
                let samples: &[f32] = unsafe {
                    std::slice::from_raw_parts(
                        buffer.as_ptr() as *const f32,
                        buffer.len() / 4,
                    )
                };

                // Write to ring buffer - block until we can write to avoid dropping samples
                let mut rb = ring_buffer.write();
                rb.write_samples(samples);
            }
            Err(e) => {
                if should_stop.load(Ordering::Relaxed) {
                    break; // Clean exit during shutdown
                }
                log::error!("Failed to read audio: {}", e);
                return Err(e.into());
            }
        }
    }
    
    log::info!("Audio capture loop exiting cleanly");
    Ok(())
}
