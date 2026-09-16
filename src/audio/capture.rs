use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Host, Stream, StreamConfig};
use parking_lot::RwLock;
use std::sync::Arc;

use super::ring_buffer::AudioRingBuffer;

/// Audio capture manager for continuous recording
pub struct AudioCapture {
    _stream: Stream,
}

impl AudioCapture {
    /// Create and start audio capture
    ///
    /// # Arguments
    /// * `device_name` - Optional device name substring to match (e.g., "monitor", "mic")
    /// * `sample_rate` - Desired sample rate (e.g., 48000)
    /// * `channels` - Number of channels (1 = mono, 2 = stereo)
    /// * `ring_buffer` - Shared ring buffer to write samples to
    pub fn new(
        device_name: Option<&str>,
        sample_rate: u32,
        channels: u16,
        ring_buffer: Arc<RwLock<AudioRingBuffer>>,
    ) -> Result<Self> {
        let host = cpal::default_host();

        // Find and select device
        let device = if let Some(name) = device_name {
            log::info!("Searching for audio input device matching '{}'", name);
            find_device_by_name(&host, name)?
        } else {
            log::info!("Using default audio input device");
            host.default_input_device()
                .ok_or_else(|| anyhow!("No default input device available"))?
        };

        log::info!("Selected device: {}", device.name()?);

        // Configure stream
        let config = StreamConfig {
            channels,
            sample_rate: cpal::SampleRate(sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        log::info!(
            "Stream config: {} channels, {}Hz, buffer={:?}",
            config.channels,
            config.sample_rate.0,
            config.buffer_size
        );

        // Build input stream
        let rb = ring_buffer.clone();
        let stream = device.build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                // Real-time audio callback - must be fast!
                // Write samples to ring buffer
                if let Some(mut buffer) = rb.try_write() {
                    buffer.write_samples(data);
                } else {
                    // Lock contention - log but don't block
                    log::warn!("Ring buffer lock contention in audio callback");
                }
            },
            |err| {
                log::error!("Audio stream error: {}", err);
            },
            None, // No timeout
        )?;

        // Start the stream
        stream.play()?;
        log::info!("Audio capture started");

        Ok(AudioCapture { _stream: stream })
    }

    /// List all available input devices
    pub fn list_devices() -> Result<Vec<String>> {
        let host = cpal::default_host();
        let devices: Result<Vec<String>> = host
            .input_devices()?
            .map(|d| d.name().map_err(|e| anyhow!(e)))
            .collect();
        devices
    }
}

/// Find an input device by name substring
fn find_device_by_name(host: &Host, name_filter: &str) -> Result<Device> {
    let devices = host.input_devices()?;

    for device in devices {
        if let Ok(device_name) = device.name() {
            // Case-insensitive substring match
            if device_name.to_lowercase().contains(&name_filter.to_lowercase()) {
                return Ok(device);
            }
        }
    }

    Err(anyhow!(
        "No input device found matching '{}'. Available devices:\n{}",
        name_filter,
        AudioCapture::list_devices()?
            .iter()
            .map(|d| format!("  - {}", d))
            .collect::<Vec<_>>()
            .join("\n")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_devices() {
        // This test will pass if there's at least one input device
        let devices = AudioCapture::list_devices();
        assert!(devices.is_ok());

        if let Ok(devs) = devices {
            println!("Available input devices:");
            for dev in devs {
                println!("  - {}", dev);
            }
        }
    }
}
