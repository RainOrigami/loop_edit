//! Drawing shared by the live and the edit waveform.

use gtk::{gdk, graphene, gsk, prelude::*};

use crate::audio::peaks::Bin;

/// Quieter than this many dB below full scale draws as silence. Voices
/// are far below full scale, so a linear scale would leave them as a thin
/// line.
const RANGE_DB: f32 = 54.0;

/// Height of a level, from 0 for silence to 1 for full scale.
pub fn level(amplitude: f32) -> f32 {
    if amplitude <= 1e-6 {
        return 0.0;
    }
    ((20.0 * amplitude.log10() + RANGE_DB) / RANGE_DB).clamp(0.0, 1.0)
}

/// Draws columns of audio, each centred at `x(i)`, mirrored around the middle
/// of `area`: the peaks in `peak`, the loudness inside them in `body`.
pub fn waveform(
    snapshot: &gtk::Snapshot,
    area: &graphene::Rect,
    columns: &[(f32, Bin)],
    peak: &gdk::RGBA,
    body: &gdk::RGBA,
) {
    if columns.is_empty() {
        return;
    }
    let middle = area.y() + area.height() / 2.0;
    let half = area.height() / 2.0 - 1.0;
    for (value, colour) in [
        (&(|b: &Bin| b.peak) as &dyn Fn(&Bin) -> f32, peak),
        (&|b: &Bin| b.rms(), body),
    ] {
        let path = gsk::PathBuilder::new();
        // Never quite nothing, so that silence still reads as a line.
        let y = |b: &Bin| (level(value(b)) * half).max(0.5);
        let (x0, b0) = &columns[0];
        path.move_to(*x0, middle - y(b0));
        for (x, b) in &columns[1..] {
            path.line_to(*x, middle - y(b));
        }
        for (x, b) in columns.iter().rev() {
            path.line_to(*x, middle + y(b));
        }
        path.close();
        snapshot.append_fill(&path.to_path(), gsk::FillRule::Winding, colour);
    }
}

/// A one pixel wide line from top to bottom of `area` at `x`.
pub fn vline(snapshot: &gtk::Snapshot, area: &graphene::Rect, x: f32, width: f32, colour: &gdk::RGBA) {
    snapshot.append_color(
        colour,
        &graphene::Rect::new(x - width / 2.0, area.y(), width, area.height()),
    );
}

pub fn alpha(colour: &gdk::RGBA, alpha: f32) -> gdk::RGBA {
    colour.with_alpha(colour.alpha() * alpha)
}

/// Time between ticks on a ruler, so that they are at least `min_px` apart
/// at `px_per_second`.
pub fn tick_step(px_per_second: f64, min_px: f64) -> f64 {
    const STEPS: [f64; 15] = [
        0.01, 0.02, 0.05, 0.1, 0.2, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0,
    ];
    STEPS
        .into_iter()
        .find(|step| step * px_per_second >= min_px)
        .unwrap_or(300.0)
}

/// `m:ss`, with as many decimals as ticks `step` apart need.
pub fn clock(seconds: f64, step: f64) -> String {
    let decimals = if step >= 1.0 {
        0
    } else if step >= 0.1 {
        1
    } else {
        2
    };
    let seconds = seconds.max(0.0);
    let minutes = (seconds / 60.0).floor();
    let rest = seconds - minutes * 60.0;
    let width = if decimals == 0 { 2 } else { 3 + decimals };
    format!("{}:{:0width$.decimals$}", minutes as u64, rest)
}

/// Text in the widget's font, smaller, with its left edge at `x` (or its
/// centre, or right edge) and its top at `y`.
pub fn label(
    widget: &impl IsA<gtk::Widget>,
    snapshot: &gtk::Snapshot,
    text: &str,
    x: f32,
    y: f32,
    align: f32,
    colour: &gdk::RGBA,
) {
    let layout = widget.create_pango_layout(Some(text));
    let mut font = widget.pango_context().font_description().unwrap_or_default();
    font.set_size(font.size() * 8 / 10);
    layout.set_font_description(Some(&font));
    let (_, logical) = layout.pixel_extents();
    snapshot.save();
    snapshot.translate(&graphene::Point::new(
        (x - logical.width() as f32 * align).round(),
        y.round(),
    ));
    snapshot.append_layout(&layout, colour);
    snapshot.restore();
}

/// The colour to draw audio in: the configured one, or the accent colour.
pub fn audio_colour(configured: Option<[u8; 4]>) -> gdk::RGBA {
    match configured {
        Some([r, g, b, a]) => gdk::RGBA::new(
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            a as f32 / 255.0,
        ),
        None => adw::StyleManager::default().accent_color_rgba(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clocks() {
        assert_eq!(clock(65.0, 5.0), "1:05");
        assert_eq!(clock(5.26, 0.5), "0:05.3");
        assert_eq!(clock(12.345, 0.05), "0:12.35");
    }
}
