use egui::{Color32, Context, Ui};

use crate::audio::WaveformCache;
use super::waveform;

/// Render the recording view (live buffer)
pub fn render(
    _ctx: &Context,
    ui: &mut Ui,
    waveform_cache: &WaveformCache,
    waveform_color: Color32,
) -> RecorderAction {
    let mut action = RecorderAction::None;

    ui.heading("Loop Recorder");
    ui.add_space(10.0);

    // Render live waveform
    waveform::render_live_waveform(ui, waveform_cache, waveform_color);

    ui.add_space(20.0);

    // Big "Edit" button
    if ui
        .button("Edit")
        .clicked()
    {
        action = RecorderAction::StartEdit;
    }

    action
}

/// Actions that can be triggered from the recorder view
#[derive(Debug, PartialEq)]
pub enum RecorderAction {
    None,
    StartEdit,
}
