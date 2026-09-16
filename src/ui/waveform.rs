use egui::{Color32, Ui};
use egui_plot::{Line, Plot, PlotPoints, VLine};

use crate::audio::WaveformCache;

/// Render live waveform (continuously updating)
pub fn render_live_waveform(ui: &mut Ui, cache: &WaveformCache, color: Color32) {
    if cache.is_empty() {
        ui.label("Waiting for audio data...");
        return;
    }

    Plot::new("live_waveform")
        .height(500.0)
        .show_axes([false, true])
        .allow_zoom(false)
        .allow_drag(false)
        .allow_scroll(false)
        .show_background(false)
        .show(ui, |plot_ui| {
            // Create min/max envelope
            let points_min: PlotPoints = cache
                .points
                .iter()
                .enumerate()
                .map(|(i, p)| [i as f64, p.min as f64])
                .collect();

            let points_max: PlotPoints = cache
                .points
                .iter()
                .enumerate()
                .map(|(i, p)| [i as f64, p.max as f64])
                .collect();

            // Draw min line
            plot_ui.line(Line::new(points_min).color(color).width(1.0).fill(0.0).fill_alpha(1.0));

            // Draw max line
            plot_ui.line(Line::new(points_max).color(color).width(1.0).fill(0.0).fill_alpha(1.0));
        });
}

/// Render edit waveform with crop markers
pub fn render_edit_waveform(
    ui: &mut Ui,
    cache: &WaveformCache,
    crop_start: f32,
    crop_end: f32,
    playback_position: Option<f32>,
    color: Color32,
) -> CropMarkerUpdate {
    let mut update = CropMarkerUpdate::None;

    if cache.is_empty() {
        ui.label("No waveform data");
        return update;
    }

    let num_points = cache.points.len();

    let response = Plot::new("edit_waveform")
        .height(500.0)
        .show_axes([true, true])
        .allow_zoom(true)
        .allow_drag(false)
        .allow_scroll(true)
        .show_background(false)
        .show(ui, |plot_ui| {
            // Create min/max envelope
            let points_min: PlotPoints = cache
                .points
                .iter()
                .enumerate()
                .map(|(i, p)| [i as f64, p.min as f64])
                .collect();

            let points_max: PlotPoints = cache
                .points
                .iter()
                .enumerate()
                .map(|(i, p)| [i as f64, p.max as f64])
                .collect();

            // Draw waveform
            plot_ui.line(Line::new(points_min).color(color).width(1.0).fill(0.0).fill_alpha(1.0));
            plot_ui.line(Line::new(points_max).color(color).width(1.0).fill(0.0).fill_alpha(1.0));

            // Draw crop markers
            let start_x = crop_start * num_points as f32;
            let end_x = crop_end * num_points as f32;

            plot_ui.vline(
                VLine::new(start_x as f64)
                    .color(Color32::from_rgb(255, 255, 0))
                    .width(2.0),
            );

            plot_ui.vline(
                VLine::new(end_x as f64)
                    .color(Color32::from_rgb(255, 255, 0))
                    .width(2.0),
            );

            // Draw playback position if playing
            if let Some(pos) = playback_position {
                let playback_x = pos * num_points as f32;
                plot_ui.vline(
                    VLine::new(playback_x as f64)
                        .color(Color32::from_rgb(255, 0, 0))
                        .width(1.0),
                );
            }
        });

    // Handle click-and-drag crop selection
    if let Some(pointer_pos) = response.response.hover_pos() {
        // Set start marker immediately on mouse down, not after drag threshold
        if ui.input(|i| i.pointer.primary_pressed()) {
            // Convert screen position to plot coordinates
            let plot_pos = response.transform.value_from_position(pointer_pos);
            let normalized = (plot_pos.x / num_points as f64).clamp(0.0, 1.0) as f32;
            update = CropMarkerUpdate::StartMoved(normalized);
        }
        // Check if we're releasing after a drag (end of selection)
        else if response.response.drag_stopped() && ui.input(|i| !i.pointer.primary_down()) {
            let plot_pos = response.transform.value_from_position(pointer_pos);
            let normalized = (plot_pos.x / num_points as f64).clamp(0.0, 1.0) as f32;
            update = CropMarkerUpdate::EndMoved(normalized);
        }
    }

    update
}

/// Result of crop marker interaction
#[derive(Debug, PartialEq)]
pub enum CropMarkerUpdate {
    None,
    StartMoved(f32),
    EndMoved(f32),
}

