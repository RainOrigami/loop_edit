use anyhow::Result;
use eframe::egui;
use parking_lot::RwLock;
use std::sync::Arc;

use crate::audio::{self, AudioCapture, AudioRingBuffer, WaveformCache};
use crate::state::{AppState, Config};
use crate::ui::{editor_view, recorder_view};
use crate::util;

pub struct LoopEditApp {
    config: Config,
    state: AppState,
    ring_buffer: Arc<RwLock<AudioRingBuffer>>,
    live_waveform_cache: Arc<RwLock<WaveformCache>>,
    _audio_capture: Option<AudioCapture>,
    _waveform_color: egui::Color32,
}

impl LoopEditApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Result<Self> {
        let config = Config::load()?;
        
        let ring_buffer = Arc::new(RwLock::new(AudioRingBuffer::new(
            config.buffer.duration_seconds,
            config.audio.sample_rate,
            config.audio.channels,
        )));

        let audio_capture = match AudioCapture::new(
            Some(&config.audio.capture_device),
            config.audio.sample_rate,
            config.audio.channels,
            ring_buffer.clone(),
        ) {
            Ok(capture) => {
                log::info!("Audio capture started successfully");
                Some(capture)
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to start audio capture: {}", e));
            }
        };

        let live_waveform_cache = Arc::new(RwLock::new(WaveformCache::new()));
        audio::processor::start_waveform_processor(
            ring_buffer.clone(),
            live_waveform_cache.clone(),
            3000,
        );

        let waveform_color = egui::Color32::from_rgba_unmultiplied(
            config.ui.waveform_color[0],
            config.ui.waveform_color[1],
            config.ui.waveform_color[2],
            config.ui.waveform_color[3],
        );

        Ok(Self {
            config,
            state: AppState::initial(),
            ring_buffer,
            live_waveform_cache,
            _audio_capture: audio_capture,
            _waveform_color: waveform_color,
        })
    }

    fn handle_transition(&mut self, new_state: AppState) {
        self.state = new_state;
    }

    fn render_ui(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::default().inner_margin(egui::Margin::same(10.0)))
            .show(ctx, |ui| {
            ui.set_max_width(ui.available_width());
            
            match &mut self.state {
                AppState::Recording { .. } => {
                    let cache = self.live_waveform_cache.read().clone();
                    let action = recorder_view::render(ctx, ui, &cache, self._waveform_color);

                    if action == recorder_view::RecorderAction::StartEdit {
                        let (buffer_snapshot, sample_rate, channels) = {
                            let rb = self.ring_buffer.read();
                            (Arc::new(rb.snapshot()), rb.sample_rate(), rb.channels())
                        };

                        let edit_waveform_cache =
                            WaveformCache::from_buffer(&buffer_snapshot, 3000, channels);

                        // Transition to editing state
                        let new_state = AppState::Editing {
                            edit_buffer: buffer_snapshot,
                            sample_rate,
                            channels,
                            crop_start: 0.0,
                            crop_end: 1.0,
                            playback: None,
                            waveform_cache: edit_waveform_cache,
                        };

                        self.handle_transition(new_state);
                    }
                }
                AppState::Editing {
                    edit_buffer,
                    sample_rate,
                    channels,
                    crop_start,
                    crop_end,
                    playback,
                    waveform_cache,
                } => {
                    let buffer_duration =
                        edit_buffer.len() as f32 / (*sample_rate as f32 * *channels as f32);

                    let playback_position = playback.as_ref().and_then(|p| {
                        if p.is_playing() {
                            Some(*crop_start + p.position() * (*crop_end - *crop_start))
                        } else {
                            None
                        }
                    });
                    
                    let is_playing = playback.as_ref().map(|p| p.is_playing()).unwrap_or(false);

                    let action = editor_view::render(
                        ctx,
                        ui,
                        buffer_duration,
                        waveform_cache,
                        crop_start,
                        crop_end,
                        playback_position,
                        self._waveform_color,
                        is_playing,
                    );

                    match action {
                        editor_view::EditorAction::BackToRecording => {
                            if let Some(ref mut pb) = playback {
                                pb.stop();
                            }
                            
                            let new_state = AppState::Recording {
                                live_waveform_cache: WaveformCache::new(),
                            };
                            self.handle_transition(new_state);
                        }
                        editor_view::EditorAction::Play => {
                            // Extract cropped region
                            let crop_start_sample = (*crop_start * edit_buffer.len() as f32) as usize;
                            let crop_end_sample = (*crop_end * edit_buffer.len() as f32) as usize;
                            let cropped_samples = edit_buffer[crop_start_sample..crop_end_sample].to_vec();
                            
                            // Create and start playback
                            match audio::playback::AudioPlayback::new(cropped_samples, *sample_rate, *channels) {
                                Ok(mut pb) => {
                                    if let Err(e) = pb.play() {
                                        log::error!("Failed to start playback: {}", e);
                                    } else {
                                        *playback = Some(pb);
                                    }
                                }
                                Err(e) => {
                                    log::error!("Failed to create playback: {}", e);
                                }
                            }
                        }
                        editor_view::EditorAction::Stop => {
                            if let Some(ref mut pb) = playback {
                                pb.stop();
                            }
                            *playback = None;
                        }
                        editor_view::EditorAction::Save => {
                            // Export cropped audio to WAV
                            let crop_start_sample = (*crop_start * edit_buffer.len() as f32) as usize;
                            let crop_end_sample = (*crop_end * edit_buffer.len() as f32) as usize;

                            let gain_linear = 10_f32.powf(self.config.output.gain_db / 20.0);
                            let gained: Vec<f32> = edit_buffer[crop_start_sample..crop_end_sample]
                                .iter()
                                .map(|s| (s * gain_linear).clamp(-1.0, 1.0))
                                .collect();
                            let cropped_samples = gained.as_slice();

                            // Generate filename
                            let filename = util::generate_filename(&self.config.output.filename_pattern);
                            let output_folder = util::expand_tilde(&self.config.output.default_folder);
                            let output_path = output_folder.join(&filename);

                            // Write WAV file
                            match util::write_wav(
                                &output_path,
                                cropped_samples,
                                *sample_rate,
                                *channels,
                                self.config.output.bit_depth,
                            ) {
                                Ok(_) => {
                                    log::info!("Successfully saved: {:?}", output_path);
                                    // TODO: Show success notification in UI
                                }
                                Err(e) => {
                                    log::error!("Failed to save WAV file: {}", e);
                                    // TODO: Show error notification in UI
                                }
                            }
                        }
                        editor_view::EditorAction::CropStartChanged => {
                            // Ensure start < end
                            if *crop_start > *crop_end {
                                *crop_start = *crop_end - 0.01;
                                *crop_start = crop_start.max(0.0);
                            }
                        }
                        editor_view::EditorAction::CropEndChanged => {
                            // Ensure end > start
                            if *crop_end < *crop_start {
                                *crop_end = *crop_start + 0.01;
                                *crop_end = crop_end.min(1.0);
                            }
                        }
                        editor_view::EditorAction::None => {}
                    }
                }
            }
        });
    }
}

impl eframe::App for LoopEditApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.render_ui(ctx);

        if ctx.input(|i| i.focused) {
            ctx.request_repaint();
        }
    }
}
