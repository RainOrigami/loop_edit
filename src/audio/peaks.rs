use std::collections::VecDeque;

/// How loud a stretch of audio is: enough to draw it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bin {
    /// Largest absolute sample, over all channels.
    pub peak: f32,
    /// Mean of the squared samples.
    pub square: f32,
}

impl Bin {
    /// The summary of interleaved samples, whatever the channel count.
    pub fn of(samples: &[f32]) -> Self {
        if samples.is_empty() {
            return Self::default();
        }
        let (peak, sum) = samples
            .iter()
            .fold((0f32, 0f32), |(peak, sum), s| (peak.max(s.abs()), sum + s * s));
        Self {
            peak,
            square: sum / samples.len() as f32,
        }
    }

    /// The summary of neighbouring bins of equal length.
    pub fn merge(bins: impl IntoIterator<Item = Bin>) -> Self {
        let (mut peak, mut sum, mut n) = (0f32, 0f32, 0);
        for bin in bins {
            peak = peak.max(bin.peak);
            sum += bin.square;
            n += 1;
        }
        Self {
            peak,
            square: if n == 0 { 0.0 } else { sum / n as f32 },
        }
    }

    pub fn rms(&self) -> f32 {
        self.square.sqrt()
    }
}

/// A fixed recording summarised in bins, so that drawing all of it does not
/// mean reading every sample.
pub struct Peaks {
    bins: Vec<Bin>,
    frames_per_bin: usize,
    channels: usize,
}

impl Peaks {
    pub fn new(samples: &[f32], channels: u16, frames_per_bin: usize) -> Self {
        let channels = channels.max(1) as usize;
        let bins = samples
            .chunks(frames_per_bin * channels)
            .map(Bin::of)
            .collect();
        Self {
            bins,
            frames_per_bin,
            channels,
        }
    }

    /// The summary of frames `start..end` of `samples`, the audio these
    /// peaks were made from.
    pub fn range(&self, samples: &[f32], start: usize, end: usize) -> Bin {
        let frames = samples.len() / self.channels;
        let (start, end) = (start.min(frames), end.min(frames));
        if end <= start {
            return Bin::default();
        }
        if end - start < 2 * self.frames_per_bin {
            return Bin::of(&samples[start * self.channels..end * self.channels]);
        }
        let first = start / self.frames_per_bin;
        let last = end.div_ceil(self.frames_per_bin).min(self.bins.len());
        Bin::merge(self.bins[first..last].iter().copied())
    }
}

/// The summary of the newest audio, fed as it arrives.
pub struct LivePeaks {
    bins: VecDeque<Bin>,
    capacity: usize,
    frames_per_bin: usize,
    channels: usize,
    /// Samples that do not fill a bin yet.
    partial: Vec<f32>,
    /// Bins made since the start, so that a picture can stay still
    /// while the audio moves through it.
    made: u64,
}

impl LivePeaks {
    pub fn new(capacity: usize, frames_per_bin: usize, channels: u16) -> Self {
        Self {
            // Silence until there is something to show, so that the newest
            // audio is always at the same end.
            bins: std::iter::repeat_n(Bin::default(), capacity).collect(),
            capacity,
            frames_per_bin,
            channels: channels.max(1) as usize,
            partial: Vec::new(),
            made: 0,
        }
    }

    pub fn push(&mut self, mut samples: &[f32]) {
        let size = self.frames_per_bin * self.channels;
        while !samples.is_empty() {
            let take = (size - self.partial.len()).min(samples.len());
            self.partial.extend_from_slice(&samples[..take]);
            samples = &samples[take..];
            if self.partial.len() == size {
                if self.bins.len() == self.capacity {
                    self.bins.pop_front();
                }
                self.bins.push_back(Bin::of(&self.partial));
                self.partial.clear();
                self.made += 1;
            }
        }
    }

    /// Oldest first.
    pub fn bins(&self) -> &VecDeque<Bin> {
        &self.bins
    }

    pub fn made(&self) -> u64 {
        self.made
    }

    /// How far into the next bin the audio is, from 0 to 1.
    pub fn progress(&self) -> f32 {
        self.partial.len() as f32 / (self.frames_per_bin * self.channels) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bins_summarise_all_channels() {
        let bin = Bin::of(&[0.5, -1.0, 0.0, 0.0]);
        assert_eq!(bin.peak, 1.0);
        assert!((bin.square - 1.25 / 4.0).abs() < 1e-6);
    }

    #[test]
    fn ranges_agree_with_the_samples() {
        let samples: Vec<f32> = (0..1000).map(|i| ((i as f32) * 0.37).sin() * (i as f32 / 1000.0)).collect();
        let peaks = Peaks::new(&samples, 2, 8);
        let exact = Bin::of(&samples[200..1000]);
        let fast = peaks.range(&samples, 100, 500);
        assert!((exact.peak - fast.peak).abs() < 1e-6);
        // Short ranges read the samples themselves.
        assert_eq!(peaks.range(&samples, 10, 12), Bin::of(&samples[20..24]));
        assert_eq!(peaks.range(&samples, 600, 700), Bin::default());
    }

    #[test]
    fn live_peaks_keep_the_newest() {
        let mut live = LivePeaks::new(3, 2, 1);
        live.push(&[1.0, 1.0, 0.5, 0.5, 0.25]);
        assert_eq!(live.made(), 2);
        assert_eq!(live.progress(), 0.5);
        live.push(&[0.25, 0.1, 0.1, 0.2, 0.2]);
        let peaks: Vec<f32> = live.bins().iter().map(|b| b.peak).collect();
        assert_eq!(peaks, vec![0.25, 0.1, 0.2]);
    }
}
