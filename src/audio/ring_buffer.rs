/// Thread-safe circular buffer for continuous audio recording
pub struct AudioRingBuffer {
    buffer: Vec<f32>,
    write_pos: usize,
    /// Samples written since creation; tells readers what is new.
    total_written: u64,
    capacity_samples: usize,
    sample_rate: u32,
    channels: u16,
}

impl AudioRingBuffer {
    /// Create a new ring buffer with specified duration
    pub fn new(duration_secs: f32, sample_rate: u32, channels: u16) -> Self {
        let capacity_samples = (duration_secs * sample_rate as f32) as usize * channels as usize;
        log::info!(
            "Creating ring buffer: {}s, {}Hz, {} channels, {} samples ({:.2} MB)",
            duration_secs,
            sample_rate,
            channels,
            capacity_samples,
            (capacity_samples * std::mem::size_of::<f32>()) as f64 / 1_048_576.0
        );

        Self {
            buffer: vec![0.0; capacity_samples],
            write_pos: 0,
            total_written: 0,
            capacity_samples,
            sample_rate,
            channels,
        }
    }

    /// Write incoming samples to the ring buffer
    /// Called from audio capture thread - must be fast!
    pub fn write_samples(&mut self, samples: &[f32]) {
        for &sample in samples {
            self.buffer[self.write_pos] = sample;
            self.write_pos = (self.write_pos + 1) % self.capacity_samples;
        }
        self.total_written += samples.len() as u64;
    }

    /// Samples written since creation.
    pub fn total_written(&self) -> u64 {
        self.total_written
    }

    /// Like `snapshot`, but leaves out the silence in front of the first
    /// sample when the buffer has not been filled yet.
    pub fn snapshot_recorded(&self) -> Vec<f32> {
        let recorded = self.total_written.min(self.capacity_samples as u64) as usize;
        self.read_latest(recorded)
    }

    /// The samples written after `since` (a `total_written` value), at most
    /// one buffer's worth, and the `total_written` to pass next time.
    pub fn read_since(&self, since: u64) -> (Vec<f32>, u64) {
        let new = self.total_written.saturating_sub(since);
        let n = new.min(self.capacity_samples as u64) as usize;
        (self.read_latest(n), self.total_written)
    }

    /// Create a snapshot of the current buffer state
    /// Returns samples in chronological order (oldest to newest)
    pub fn snapshot(&self) -> Vec<f32> {
        let mut result = Vec::with_capacity(self.capacity_samples);

        // Copy from write_pos to end (oldest data)
        result.extend_from_slice(&self.buffer[self.write_pos..]);
        // Copy from start to write_pos (newest data)
        result.extend_from_slice(&self.buffer[..self.write_pos]);

        result
    }

    /// Read the latest N samples for live visualization
    /// Returns samples in chronological order
    pub fn read_latest(&self, num_samples: usize) -> Vec<f32> {
        let num_samples = num_samples.min(self.capacity_samples);
        let mut result = Vec::with_capacity(num_samples);

        // Calculate start position (N samples back from write position)
        let start = if self.write_pos >= num_samples {
            self.write_pos - num_samples
        } else {
            self.capacity_samples - (num_samples - self.write_pos)
        };

        // Read in circular fashion
        let first = (self.capacity_samples - start).min(num_samples);
        result.extend_from_slice(&self.buffer[start..start + first]);
        result.extend_from_slice(&self.buffer[..num_samples - first]);

        result
    }

    /// Get buffer metadata
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }

    pub fn capacity_samples(&self) -> usize {
        self.capacity_samples
    }
    
    pub fn write_pos(&self) -> usize {
        self.write_pos
    }

    pub fn duration_secs(&self) -> f32 {
        self.capacity_samples as f32 / (self.sample_rate as f32 * self.channels as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_buffer_creation() {
        let buf = AudioRingBuffer::new(10.0, 48000, 2);
        assert_eq!(buf.sample_rate(), 48000);
        assert_eq!(buf.channels(), 2);
        assert_eq!(buf.capacity_samples(), 960000);
        assert!((buf.duration_secs() - 10.0).abs() < 0.01);
    }

    #[test]
    fn test_write_and_read() {
        let mut buf = AudioRingBuffer::new(1.0, 100, 1);

        // Write some samples
        let samples: Vec<f32> = (0..50).map(|i| i as f32).collect();
        buf.write_samples(&samples);

        // Read them back
        let read = buf.read_latest(50);
        assert_eq!(read.len(), 50);
        assert_eq!(read, samples);
    }

    #[test]
    fn test_wrap_around() {
        let mut buf = AudioRingBuffer::new(1.0, 10, 1); // 10 sample capacity

        // Write 15 samples (will wrap around)
        let samples: Vec<f32> = (0..15).map(|i| i as f32).collect();
        buf.write_samples(&samples);

        // Should contain the last 10 samples: [5, 6, 7, 8, 9, 10, 11, 12, 13, 14]
        let snapshot = buf.snapshot();
        assert_eq!(snapshot.len(), 10);
        assert_eq!(snapshot[0], 5.0); // Oldest
        assert_eq!(snapshot[9], 14.0); // Newest
    }

    #[test]
    fn test_snapshot_chronological_order() {
        let mut buf = AudioRingBuffer::new(1.0, 10, 1);

        // Write samples that will wrap
        buf.write_samples(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]);
        buf.write_samples(&[11.0, 12.0, 13.0]);

        // Snapshot should be in order: [4, 5, 6, 7, 8, 9, 10, 11, 12, 13]
        let snapshot = buf.snapshot();
        for i in 0..9 {
            assert!(snapshot[i] < snapshot[i + 1], "Not in chronological order");
        }
    }

    #[test]
    fn test_read_since_and_recorded() {
        let mut buf = AudioRingBuffer::new(1.0, 10, 1);
        buf.write_samples(&[1.0, 2.0, 3.0]);
        assert_eq!(buf.snapshot_recorded(), vec![1.0, 2.0, 3.0]);
        let (new, at) = buf.read_since(0);
        assert_eq!((new, at), (vec![1.0, 2.0, 3.0], 3));
        buf.write_samples(&(4..=15).map(|i| i as f32).collect::<Vec<_>>());
        // More arrived than fits: only the latest buffer's worth.
        let (new, at) = buf.read_since(at);
        assert_eq!(new.len(), 10);
        assert_eq!((new[0], at), (6.0, 15));
        assert_eq!(buf.read_since(at).0.len(), 0);
    }

    #[test]
    fn test_read_latest() {
        let mut buf = AudioRingBuffer::new(1.0, 100, 1);
        buf.write_samples(&(0..100).map(|i| i as f32).collect::<Vec<_>>());

        // Read last 10 samples
        let last_10 = buf.read_latest(10);
        assert_eq!(last_10.len(), 10);
        assert_eq!(last_10[0], 90.0);
        assert_eq!(last_10[9], 99.0);
    }
}
