//! The window: the live recording on one page, a capture being cut on the
//! other.

mod draw;
mod edit;
mod live;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use gtk::{gio, glib};
use parking_lot::RwLock;

use crate::audio::playback::AudioPlayback;
use crate::audio::trim;
use crate::audio::{AudioCapture, AudioRingBuffer};
use crate::state::Config;
use crate::util;
use edit::EditWave;
use live::LiveWave;

/// How much of the end of a capture is selected to begin with.
const FIRST_SELECTION: f64 = 10.0;

pub const CSS: &str = "
wave {
  background-color: mix(@window_bg_color, @window_fg_color, 0.045);
  border-radius: 12px;
}
.rec-dot { color: #e01b24; font-size: 1.1em; animation: rec 1.6s ease-in-out infinite; }
@keyframes rec { 50% { opacity: 0.25; } }
.capture { padding: 10px 28px; font-size: 1.15em; }
.stat-label { font-size: 0.8em; font-weight: bold; opacity: 0.55; }
.stat-value { font-size: 1.15em; font-feature-settings: 'tnum'; }
";

/// Everything the window works with.
struct App {
    config: Config,
    ring: Arc<RwLock<AudioRingBuffer>>,
    capture: AudioCapture,
    playback: RefCell<Option<AudioPlayback>>,
    /// Where the current playback starts, in frames of the capture.
    played_from: Cell<usize>,
    window: adw::ApplicationWindow,
    stack: gtk::Stack,
    toasts: adw::ToastOverlay,
    title: adw::WindowTitle,
    back: gtk::Button,
    rec: gtk::Label,
    wave: EditWave,
    play: gtk::Button,
    start: gtk::Label,
    end: gtk::Label,
    length: gtk::Label,
}

pub fn build(app: &adw::Application) -> adw::ApplicationWindow {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Loop Edit")
        .default_width(1000)
        .default_height(600)
        .width_request(560)
        .height_request(420)
        .build();
    match start(app, &window) {
        Ok(content) => window.set_content(Some(&content)),
        Err(e) => window.set_content(Some(&failure(&format!("{e:#}")))),
    }
    window
}

/// The window when there is nothing to record from.
fn failure(message: &str) -> gtk::Widget {
    let page = adw::StatusPage::builder()
        .icon_name("dialog-error-symbolic")
        .title("Can't Start Recording")
        .description(glib::markup_escape_text(message).as_str())
        .build();
    page.upcast()
}

fn start(app: &adw::Application, window: &adw::ApplicationWindow) -> anyhow::Result<gtk::Widget> {
    let config = Config::load()?;
    let ring = Arc::new(RwLock::new(AudioRingBuffer::new(
        config.buffer.duration_seconds,
        config.audio.sample_rate,
        config.audio.channels,
    )));
    let capture = AudioCapture::new(
        Some(&config.audio.capture_device),
        config.audio.sample_rate,
        config.audio.channels,
        ring.clone(),
    )?;
    let colour = config.ui.waveform_color.map(|c| draw::audio_colour(Some(c)));

    // A bar with the page's title, a way back and the output folder. Not a
    // header bar: moving, resizing and closing are the compositor's job.
    let title = adw::WindowTitle::new("Loop Edit", "");
    let start_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let header = gtk::CenterBox::new();
    header.set_margin_top(8);
    header.set_margin_start(12);
    header.set_margin_end(12);
    header.set_start_widget(Some(&start_box));
    header.set_center_widget(Some(&title));
    let back = gtk::Button::from_icon_name("go-previous-symbolic");
    back.set_tooltip_text(Some("Back to Recording (Esc)"));
    back.set_action_name(Some("win.back"));
    back.set_visible(false);
    back.add_css_class("flat");
    start_box.append(&back);
    let rec = gtk::Label::new(Some("●"));
    rec.add_css_class("rec-dot");
    rec.set_tooltip_text(Some("Recording"));
    rec.set_margin_start(6);
    start_box.append(&rec);
    let folder = gtk::Button::from_icon_name("folder-open-symbolic");
    folder.set_tooltip_text(Some("Open Output Folder"));
    folder.set_action_name(Some("win.folder"));
    folder.add_css_class("flat");
    header.set_end_widget(Some(&folder));

    // Recording page.
    let live = LiveWave::new(ring.clone(), colour);
    live.set_vexpand(true);
    let capture_button = gtk::Button::builder()
        .label("Capture")
        .action_name("win.capture")
        .tooltip_text("Edit the last seconds (Space)")
        .halign(gtk::Align::Center)
        .build();
    capture_button.add_css_class("suggested-action");
    capture_button.add_css_class("pill");
    capture_button.add_css_class("capture");
    let seconds = config.buffer.duration_seconds;
    let hint = gtk::Label::new(Some(&format!(
        "Takes the last {} seconds. Recording carries on.",
        seconds.round()
    )));
    hint.add_css_class("dim-label");
    hint.add_css_class("caption");
    let recording = gtk::Box::new(gtk::Orientation::Vertical, 12);
    recording.append(&live);
    recording.append(&capture_button);
    recording.append(&hint);

    // Editing page.
    let wave = EditWave::new(colour);
    wave.set_vexpand(true);
    let (start, start_box) = stat("Start");
    let (end, end_box) = stat("End");
    let (length, length_box) = stat("Length");
    let stats = gtk::Box::new(gtk::Orientation::Horizontal, 28);
    stats.append(&start_box);
    stats.append(&end_box);
    stats.append(&length_box);
    stats.set_hexpand(true);
    let zoom = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    zoom.add_css_class("linked");
    for (icon, action, tip) in [
        ("zoom-fit-best-symbolic", "win.zoom-fit", "Show All (0)"),
        ("zoom-in-symbolic", "win.zoom-selection", "Zoom to Selection (Z)"),
        ("edit-cut-symbolic", "win.trim", "Trim Silence (T)"),
    ] {
        let button = gtk::Button::from_icon_name(icon);
        button.set_action_name(Some(action));
        button.set_tooltip_text(Some(tip));
        zoom.append(&button);
    }
    let play = gtk::Button::builder()
        .action_name("win.play")
        .tooltip_text("Play Selection (Space)")
        .build();
    play.add_css_class("circular");
    play.add_css_class("raised");
    let save = gtk::Button::builder()
        .label("Save WAV")
        .action_name("win.save")
        .tooltip_text("Save the selection (Ctrl+S)")
        .build();
    save.add_css_class("suggested-action");
    save.add_css_class("pill");
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    bar.append(&stats);
    bar.append(&zoom);
    bar.append(&play);
    bar.append(&save);
    let editing = gtk::Box::new(gtk::Orientation::Vertical, 14);
    editing.append(&wave);
    editing.append(&bar);

    for page in [&recording, &editing] {
        page.set_margin_top(6);
        page.set_margin_bottom(18);
        page.set_margin_start(18);
        page.set_margin_end(18);
    }
    let stack = gtk::Stack::new();
    stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    stack.add_named(&recording, Some("recording"));
    stack.add_named(&editing, Some("editing"));

    // Capture dies quietly in its own thread; say so where it shows.
    let banner = adw::Banner::new("");
    let toasts = adw::ToastOverlay::new();
    toasts.set_child(Some(&stack));
    let view = adw::ToolbarView::new();
    view.add_top_bar(&header);
    view.add_top_bar(&banner);
    view.set_content(Some(&toasts));

    let app_state = Rc::new(App {
        config,
        ring,
        capture,
        playback: RefCell::default(),
        played_from: Cell::new(0),
        window: window.clone(),
        stack,
        toasts,
        title,
        back,
        rec,
        wave,
        play,
        start,
        end,
        length,
    });
    let app_ = app_state.clone();
    app_state.wave.connect_selection_changed(move || app_.show_selection());
    let app_ = Rc::downgrade(&app_state);
    glib::timeout_add_seconds_local(1, move || {
        let Some(app) = app_.upgrade() else {
            return glib::ControlFlow::Break;
        };
        if let Some(e) = app.capture.error() {
            banner.set_title(&format!("Recording stopped: {e}"));
            banner.set_revealed(true);
            app.rec.set_visible(false);
            return glib::ControlFlow::Break;
        }
        glib::ControlFlow::Continue
    });
    // The playhead follows what is heard, every frame.
    let app_ = Rc::downgrade(&app_state);
    app_state.wave.add_tick_callback(move |_, _| {
        if let Some(app) = app_.upgrade() {
            app.follow_playback();
        }
        glib::ControlFlow::Continue
    });
    actions(app, &app_state);
    app_state.show_recording();
    Ok(view.upcast())
}

/// A small caption over a number.
fn stat(caption: &str) -> (gtk::Label, gtk::Box) {
    let label = gtk::Label::new(Some(caption));
    label.add_css_class("stat-label");
    label.set_xalign(0.0);
    let value = gtk::Label::new(None);
    value.add_css_class("stat-value");
    value.set_xalign(0.0);
    let column = gtk::Box::new(gtk::Orientation::Vertical, 2);
    column.set_valign(gtk::Align::Center);
    column.append(&label);
    column.append(&value);
    (value, column)
}

fn actions(app: &adw::Application, state: &Rc<App>) {
    let entries: [(&str, &[&str], fn(&App)); 9] = [
        ("capture", &["space", "Return"], App::capture),
        ("back", &["Escape"], App::show_recording),
        ("play", &["space"], App::toggle_playback),
        ("save", &["<Ctrl>s"], App::save),
        ("zoom-fit", &["0"], |app| app.wave.zoom_fit()),
        ("zoom-selection", &["z"], |app| app.wave.zoom_selection()),
        ("select-all", &["<Ctrl>a"], |app| app.wave.set_selection(0.0, f64::MAX)),
        ("trim", &["t"], App::trim),
        ("folder", &[], App::open_folder),
    ];
    for (name, keys, run) in entries {
        let action = gio::SimpleAction::new(name, None);
        let weak = Rc::downgrade(state);
        action.connect_activate(move |_, _| {
            if let Some(state) = weak.upgrade() {
                run(&state);
            }
        });
        state.window.add_action(&action);
        app.set_accels_for_action(&format!("win.{name}"), keys);
    }
}

impl App {
    fn action(&self, name: &str) -> Option<gio::SimpleAction> {
        self.window
            .lookup_action(name)
            .and_then(|a| a.downcast().ok())
    }

    /// Switches on the actions of one page and off those of the other, so
    /// that Space means the right thing.
    fn enable(&self, editing: bool) {
        for name in ["capture"] {
            if let Some(a) = self.action(name) {
                a.set_enabled(!editing);
            }
        }
        for name in ["back", "play", "save", "zoom-fit", "zoom-selection", "select-all", "trim"] {
            if let Some(a) = self.action(name) {
                a.set_enabled(editing);
            }
        }
    }

    fn show_recording(&self) {
        self.stop_playback();
        self.stack.set_visible_child_name("recording");
        self.title.set_title("Loop Edit");
        self.title
            .set_subtitle(&format!("Recording {}", self.config.audio.capture_device));
        self.back.set_visible(false);
        self.enable(false);
    }

    fn capture(&self) {
        let (samples, rate, channels) = {
            let ring = self.ring.read();
            (ring.snapshot_recorded(), ring.sample_rate(), ring.channels())
        };
        if samples.is_empty() {
            self.toast("Nothing recorded yet");
            return;
        }
        let frames = (samples.len() / channels.max(1) as usize) as f64;
        let first = (frames - FIRST_SELECTION * rate as f64).max(0.0);
        self.wave
            .set_audio(Rc::new(samples), rate, channels, (first, frames));
        self.title.set_title("Edit Capture");
        self.title.set_subtitle(&format!(
            "{} · {:.1} s",
            chrono::Local::now().format("Captured %H:%M:%S"),
            frames / rate as f64
        ));
        self.back.set_visible(true);
        self.stack.set_visible_child_name("editing");
        self.wave.grab_focus();
        self.enable(true);
        self.show_playing(false);
    }

    fn show_selection(&self) {
        let rate = self.wave.rate();
        let (a, b) = self.wave.selection();
        let time = |frames: usize| draw::clock(frames as f64 / rate, 0.001);
        self.start.set_label(&time(a));
        self.end.set_label(&time(b));
        self.length
            .set_label(&format!("{:.2} s", (b - a) as f64 / rate));
        if let Some(save) = self.action("save") {
            save.set_enabled(b > a && self.stack.visible_child_name().as_deref() == Some("editing"));
        }
        // Playing an old selection would be confusing.
        if self.playback.borrow().is_some() {
            self.stop_playback();
        }
    }

    /// Snaps the selection to the sound in it.
    fn trim(&self) {
        let (a, b) = self.wave.selection();
        let samples = self.wave.samples();
        let channels = self.config.audio.channels;
        match trim::trim(&samples, channels, self.wave.rate() as u32, a, b) {
            Some((a, b)) => self.wave.set_selection(a as f64, b as f64),
            None => self.toast("Only silence selected"),
        }
    }

    fn toggle_playback(&self) {
        if self.playback.borrow().as_ref().is_some_and(|p| p.is_playing()) {
            self.stop_playback();
            return;
        }
        let (a, b) = self.wave.selection();
        if b <= a {
            return;
        }
        let channels = self.config.audio.channels.max(1) as usize;
        let samples = self.wave.samples()[a * channels..b * channels].to_vec();
        let started = AudioPlayback::new(samples, self.wave.rate() as u32, channels as u16)
            .and_then(|mut p| p.play().map(|_| p));
        match started {
            Ok(playback) => {
                self.played_from.set(a);
                self.playback.replace(Some(playback));
                self.show_playing(true);
            }
            Err(e) => self.toast(&format!("Can't play: {e}")),
        }
    }

    fn stop_playback(&self) {
        if let Some(mut playback) = self.playback.take() {
            playback.stop();
        }
        self.wave.set_playhead(None);
        self.show_playing(false);
    }

    fn follow_playback(&self) {
        let playing = self.playback.borrow().as_ref().map(|p| (p.is_playing(), p.position()));
        match playing {
            Some((true, position)) => {
                let (a, b) = self.wave.selection();
                let from = self.played_from.get() as f64;
                self.wave
                    .set_playhead(Some(from + position as f64 * (b - a) as f64));
            }
            Some((false, _)) => self.stop_playback(),
            None => {}
        }
    }

    fn show_playing(&self, playing: bool) {
        self.play.set_icon_name(if playing {
            "media-playback-stop-symbolic"
        } else {
            "media-playback-start-symbolic"
        });
        self.play.set_tooltip_text(Some(if playing {
            "Stop (Space)"
        } else {
            "Play Selection (Space)"
        }));
    }

    fn save(&self) {
        let (a, b) = self.wave.selection();
        if b <= a {
            return;
        }
        let output = &self.config.output;
        let channels = self.config.audio.channels.max(1) as usize;
        let gain = 10f32.powf(output.gain_db / 20.0);
        let samples: Vec<f32> = self.wave.samples()[a * channels..b * channels]
            .iter()
            .map(|s| (s * gain).clamp(-1.0, 1.0))
            .collect();
        let path = util::expand_tilde(&output.default_folder)
            .join(util::generate_filename(&output.filename_pattern));
        match util::write_wav(
            &path,
            &samples,
            self.wave.rate() as u32,
            channels as u16,
            output.bit_depth,
        ) {
            Ok(()) => {
                log::info!("Saved {}", path.display());
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                let toast = adw::Toast::builder()
                    .title(glib::markup_escape_text(&format!("Saved {name}")).as_str())
                    .button_label("Show")
                    .timeout(4)
                    .build();
                let window = self.window.clone();
                toast.connect_button_clicked(move |_| {
                    gtk::FileLauncher::new(Some(&gio::File::for_path(&path))).open_containing_folder(
                        Some(&window),
                        gio::Cancellable::NONE,
                        |_| {},
                    );
                });
                self.toasts.add_toast(toast);
            }
            Err(e) => {
                log::error!("Failed to save {}: {e:#}", path.display());
                self.toast(&format!("Can't save: {e}"));
            }
        }
    }

    fn open_folder(&self) {
        let folder = util::expand_tilde(&self.config.output.default_folder);
        let _ = std::fs::create_dir_all(&folder);
        gtk::FileLauncher::new(Some(&gio::File::for_path(&folder))).launch(
            Some(&self.window),
            gio::Cancellable::NONE,
            |_| {},
        );
    }

    fn toast(&self, text: &str) {
        self.toasts
            .add_toast(adw::Toast::new(glib::markup_escape_text(text).as_str()));
    }
}
