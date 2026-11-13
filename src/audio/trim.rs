//! Snapping a rough selection to where the sound in it starts and ends.

/// Length of the windows loudness is measured over.
const WINDOW_SECONDS: f64 = 0.01;
/// How far an edge may move outwards to find silence.
const MAX_GROW_SECONDS: f64 = 2.0;
/// Kept before and after the sound, so that its attack and tail survive.
const PAD_SECONDS: f64 = 0.015;
/// Never silence above this, never sound below this.
const LOUDEST_SILENCE_DB: f32 = -25.0;
const QUIETEST_SOUND_DB: f32 = -55.0;
/// Silence is this far below the loudest part of the selection.
const SILENCE_BELOW_PEAK_DB: f32 = 30.0;

/// Moves each edge of the selection `start..end` (in frames) to the nearest
/// border between silence and sound: inwards if it is in silence, outwards
/// if it is in the middle of a sound. `None` if the selection is silent.
pub fn trim(samples: &[f32], channels: u16, rate: u32, start: usize, end: usize) -> Option<(usize, usize)> {
    let channels = channels.max(1) as usize;
    let frames = samples.len() / channels;
    let window = ((rate as f64 * WINDOW_SECONDS) as usize).max(1);
    let levels: Vec<f32> = samples
        .chunks(window * channels)
        .map(|chunk| {
            let square = chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32;
            10.0 * square.max(1e-12).log10()
        })
        .collect();
    let (first, last) = (start / window, end.min(frames).div_ceil(window).min(levels.len()));
    if first >= last {
        return None;
    }
    let peak = levels[first..last].iter().copied().fold(f32::MIN, f32::max);
    if peak < QUIETEST_SOUND_DB {
        return None;
    }
    let threshold = (peak - SILENCE_BELOW_PEAK_DB).clamp(QUIETEST_SOUND_DB, LOUDEST_SILENCE_DB);
    let loud = |i: usize| levels[i] > threshold;
    let grow = (MAX_GROW_SECONDS / WINDOW_SECONDS) as usize;

    let mut a = first;
    if loud(a) {
        while a > 0 && loud(a - 1) && first - a < grow {
            a -= 1;
        }
    } else {
        while a < last && !loud(a) {
            a += 1;
        }
    }
    let mut b = last - 1;
    if loud(b) {
        while b + 1 < levels.len() && loud(b + 1) && b + 1 - (last - 1) < grow {
            b += 1;
        }
    } else {
        while b > a && !loud(b) {
            b -= 1;
        }
    }
    let pad = (rate as f64 * PAD_SECONDS) as usize;
    let from = (a * window).saturating_sub(pad);
    let to = ((b + 1) * window + pad).min(frames);
    (to > from).then_some((from, to))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mono at 1000 Hz: silence, a tone from 1.0 s to 2.0 s, silence.
    fn blip() -> Vec<f32> {
        (0..3000)
            .map(|i| {
                if (1000..2000).contains(&i) {
                    ((i as f32) * 0.7).sin() * 0.5
                } else {
                    0.0005 * ((i as f32) * 1.3).sin()
                }
            })
            .collect()
    }

    #[test]
    fn shrinks_onto_the_sound() {
        let (a, b) = trim(&blip(), 1, 1000, 300, 2700).unwrap();
        assert!((980..=1000).contains(&a), "{a}");
        assert!((2000..=2020).contains(&b), "{b}");
    }

    #[test]
    fn grows_out_of_the_sound() {
        let (a, b) = trim(&blip(), 1, 1000, 1300, 1700).unwrap();
        assert!((980..=1000).contains(&a), "{a}");
        assert!((2000..=2020).contains(&b), "{b}");
    }

    #[test]
    fn silence_has_nothing_to_keep() {
        assert_eq!(trim(&blip(), 1, 1000, 100, 900), None);
    }
}
