use anyhow::{Context, Result};
use chrono::Local;
use hound::{WavSpec, WavWriter};
use std::fs;
use std::path::{Path, PathBuf};

/// Write audio samples to a WAV file
pub fn write_wav(
    path: &Path,
    samples: &[f32],
    sample_rate: u32,
    channels: u16,
    bit_depth: u16,
) -> Result<()> {
    // Create parent directory if it doesn't exist
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {:?}", parent))?;
    }

    // Configure WAV spec
    let spec = WavSpec {
        channels,
        sample_rate,
        bits_per_sample: bit_depth,
        sample_format: if bit_depth == 32 {
            hound::SampleFormat::Float
        } else {
            hound::SampleFormat::Int
        },
    };

    // Create writer
    let mut writer = WavWriter::create(path, spec)
        .with_context(|| format!("Failed to create WAV file: {:?}", path))?;

    // Write samples
    match bit_depth {
        16 => {
            for &sample in samples {
                // Convert f32 (-1.0 to 1.0) to i16
                let sample_i16 = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                writer.write_sample(sample_i16)
                    .context("Failed to write sample")?;
            }
        }
        24 => {
            for &sample in samples {
                // Convert f32 to i24 (represented as i32)
                let sample_i24 = (sample.clamp(-1.0, 1.0) * 8388607.0) as i32;
                writer.write_sample(sample_i24)
                    .context("Failed to write sample")?;
            }
        }
        32 => {
            for &sample in samples {
                writer.write_sample(sample)
                    .context("Failed to write sample")?;
            }
        }
        _ => {
            anyhow::bail!("Unsupported bit depth: {}. Use 16, 24, or 32", bit_depth);
        }
    }

    writer.finalize()
        .context("Failed to finalize WAV file")?;

    log::info!("Wrote WAV file: {:?} ({} samples, {}Hz, {} channels, {}bit)",
        path, samples.len(), sample_rate, channels, bit_depth);

    Ok(())
}

/// Generate a filename from a pattern
pub fn generate_filename(pattern: &str) -> String {
    let now = Local::now();
    now.format(pattern).to_string()
}

/// Expand ~ in path to home directory
pub fn expand_tilde(path: &str) -> PathBuf {
    if path.starts_with("~/") {
        if let Some(home) = std::env::var("HOME").ok() {
            return PathBuf::from(home).join(&path[2..]);
        }
    }
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_write_wav() {
        let temp_path = PathBuf::from("/tmp/test_loop_edit.wav");

        // Generate test samples (1 second of 440Hz sine wave)
        let sample_rate = 48000;
        let frequency = 440.0;
        let duration = 1.0;
        let num_samples = (sample_rate as f32 * duration) as usize * 2; // stereo

        let mut samples = Vec::with_capacity(num_samples);
        for i in 0..num_samples / 2 {
            let t = i as f32 / sample_rate as f32;
            let sample = (2.0 * std::f32::consts::PI * frequency * t).sin() * 0.5;
            samples.push(sample); // Left channel
            samples.push(sample); // Right channel
        }

        // Write WAV file
        let result = write_wav(&temp_path, &samples, sample_rate, 2, 16);
        assert!(result.is_ok());

        // Verify file exists
        assert!(temp_path.exists());

        // Clean up
        fs::remove_file(temp_path).ok();
    }

    #[test]
    fn test_generate_filename() {
        let filename = generate_filename("loop_%Y%m%d_%H%M%S.wav");
        assert!(filename.starts_with("loop_"));
        assert!(filename.ends_with(".wav"));
    }
}
