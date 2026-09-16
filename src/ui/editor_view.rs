use egui::{Color32, Context, Ui};

use crate::audio::WaveformCache;
use super::waveform;

/// Render the editor view (editing a snapshot)
pub fn render(
    _ctx: &Context,
    ui: &mut Ui,
    buffer_duration: f32,
    waveform_cache: &WaveformCache,
    crop_start: &mut f32,
    crop_end: &mut f32,
    playback_position: Option<f32>,
    waveform_color: Color32,
    is_playing: bool,
) -> EditorAction {
    let mut action = EditorAction::None;

    ui.heading("Audio Editor");
    ui.add_space(10.0);

    // Calculate crop times
    let crop_start_time = *crop_start * buffer_duration;
    let crop_end_time = *crop_end * buffer_duration;
    let crop_duration = crop_end_time - crop_start_time;

    ui.label(format!(
        "Crop: {:.2}s - {:.2}s ({:.2}s duration)",
        crop_start_time, crop_end_time, crop_duration
    ));

    ui.add_space(10.0);

    // Render waveform with crop markers and handle click-drag selection
    let crop_update = waveform::render_edit_waveform(
        ui,
        waveform_cache,
        *crop_start,
        *crop_end,
        playback_position,
        waveform_color,
    );

    // Handle crop marker updates from waveform interaction
    match crop_update {
        waveform::CropMarkerUpdate::StartMoved(pos) => {
            *crop_start = pos;
            action = EditorAction::CropStartChanged;
        }
        waveform::CropMarkerUpdate::EndMoved(pos) => {
            *crop_end = pos;
            action = EditorAction::CropEndChanged;
        }
        waveform::CropMarkerUpdate::None => {}
    }

    ui.add_space(10.0);

    // Crop sliders with better precision and full width
    ui.label("Adjust crop region:");
    ui.add_space(5.0);
    
    // Override slider width to be full width, leaving room for value display
    let full_width = ui.available_width();
    ui.spacing_mut().slider_width = full_width - 80.0; // Leave space for value text
    
    // Start slider
    ui.label("Start:");
    if ui.add(
        egui::Slider::new(crop_start, 0.0..=1.0)
            .step_by(0.0001)
            .custom_formatter(|n, _| format!("{:.2}s", n * buffer_duration as f64))
    ).changed() {
        action = EditorAction::CropStartChanged;
    }
    
    ui.add_space(3.0);
    
    // End slider
    ui.label("End:");
    if ui.add(
        egui::Slider::new(crop_end, 0.0..=1.0)
            .step_by(0.0001)
            .custom_formatter(|n, _| format!("{:.2}s", n * buffer_duration as f64))
    ).changed() {
        action = EditorAction::CropEndChanged;
    }

    ui.add_space(10.0);

    // Control buttons
    ui.horizontal(|ui| {
        if ui.button("Back").clicked() {
            action = EditorAction::BackToRecording;
        }

        ui.add_space(10.0);

        // Play/Stop button
        if is_playing {
            if ui.button("Stop Crop").clicked() {
                action = EditorAction::Stop;
            }
        } else {
            if ui.button("Play Crop").clicked() {
                action = EditorAction::Play;
            }
        }

        ui.add_space(10.0);

        if ui.button("Save WAV").clicked() {
            action = EditorAction::Save;
        }
    });

    action
}

/// Actions that can be triggered from the editor view
#[derive(Debug, PartialEq)]
pub enum EditorAction {
    None,
    BackToRecording,
    Save,
    Play,
    Stop,
    CropStartChanged,
    CropEndChanged,
}
