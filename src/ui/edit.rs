//! The waveform of a capture, with the part to keep selected on it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::{gdk, glib, graphene, gsk, prelude::*, subclass::prelude::*};

use super::draw;
use crate::audio::peaks::{Bin, Peaks};

/// Room under the waveform for the time labels.
const RULER: f32 = 22.0;
/// Height of the strip showing the whole capture.
const OVERVIEW: f32 = 40.0;
/// Space around the overview strip.
const INSET: f32 = 8.0;
/// How near a selection edge the pointer has to be to take hold of it.
const GRAB: f64 = 8.0;
/// The closest one can zoom in, in seconds across the widget.
const MIN_VIEW: f64 = 0.02;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Drag {
    #[default]
    None,
    /// Not a selection until the pointer has moved a little.
    New { anchor: f64 },
    Start,
    End,
    Overview { grab: f64 },
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct EditWave {
        pub samples: RefCell<Rc<Vec<f32>>>,
        pub peaks: RefCell<Option<Peaks>>,
        pub channels: Cell<usize>,
        pub rate: Cell<f64>,
        /// Everything below is in frames.
        pub frames: Cell<f64>,
        pub view: Cell<(f64, f64)>,
        pub selection: Cell<(f64, f64)>,
        pub playhead: Cell<Option<f64>>,
        pub drag: Cell<Drag>,
        pub drag_x: Cell<f64>,
        pub pointer: Cell<(f64, f64)>,
        pub colour: Cell<Option<gdk::RGBA>>,
        pub changed: RefCell<Vec<Box<dyn Fn()>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for EditWave {
        const NAME: &'static str = "LoopEditEditWave";
        type Type = super::EditWave;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("wave");
        }
    }

    impl ObjectImpl for EditWave {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.set_focusable(true);

            let drag = gtk::GestureDrag::new();
            drag.connect_drag_begin(glib::clone!(
                #[weak]
                obj,
                move |_, x, y| obj.imp().drag_begin(x, y)
            ));
            drag.connect_drag_update(glib::clone!(
                #[weak]
                obj,
                move |gesture, dx, _| {
                    if let Some((x, _)) = gesture.start_point() {
                        obj.imp().drag_to(x + dx, dx);
                    }
                }
            ));
            drag.connect_drag_end(glib::clone!(
                #[weak]
                obj,
                move |_, _, _| {
                    obj.imp().drag.set(Drag::None);
                    obj.imp().update_cursor();
                }
            ));
            obj.add_controller(drag);

            // A double click selects everything.
            let click = gtk::GestureClick::new();
            click.connect_pressed(glib::clone!(
                #[weak]
                obj,
                move |_, n, _, y| {
                    obj.grab_focus();
                    if n == 2 && (y as f32) < obj.imp().overview().y() {
                        obj.set_selection(0.0, obj.imp().frames.get());
                    }
                }
            ));
            obj.add_controller(click);

            let motion = gtk::EventControllerMotion::new();
            motion.connect_motion(glib::clone!(
                #[weak]
                obj,
                move |_, x, y| {
                    obj.imp().pointer.set((x, y));
                    obj.imp().update_cursor();
                }
            ));
            obj.add_controller(motion);

            // The wheel zooms around the pointer; sideways, or with Shift,
            // it moves along.
            let scroll = gtk::EventControllerScroll::new(
                gtk::EventControllerScrollFlags::BOTH_AXES,
            );
            scroll.connect_scroll(glib::clone!(
                #[weak]
                obj,
                #[upgrade_or]
                glib::Propagation::Proceed,
                move |scroll, dx, dy| {
                    let imp = obj.imp();
                    let wheel = scroll.unit() == gdk::ScrollUnit::Wheel;
                    let shift = scroll
                        .current_event_state()
                        .contains(gdk::ModifierType::SHIFT_MASK);
                    let (dx, dy) = if shift { (dx + dy, 0.0) } else { (dx, dy) };
                    let (v0, v1) = imp.view.get();
                    let width = obj.width().max(1) as f64;
                    if dx != 0.0 {
                        let px = if wheel { dx * width * 0.1 } else { dx };
                        imp.set_view(v0 + px * (v1 - v0) / width, v1 + px * (v1 - v0) / width);
                    }
                    if dy != 0.0 {
                        let factor = if wheel { 1.25f64.powf(dy) } else { (dy * 0.01).exp() };
                        obj.zoom(factor, imp.pointer.get().0);
                    }
                    glib::Propagation::Stop
                }
            ));
            obj.add_controller(scroll);
        }
    }

    impl EditWave {
        pub fn wave_area(&self) -> graphene::Rect {
            let obj = self.obj();
            let height = obj.height() as f32 - OVERVIEW - INSET - RULER - 4.0;
            graphene::Rect::new(0.0, 0.0, obj.width() as f32, height.max(0.0))
        }

        pub fn overview(&self) -> graphene::Rect {
            let obj = self.obj();
            let (width, height) = (obj.width() as f32, obj.height() as f32);
            graphene::Rect::new(
                INSET,
                height - OVERVIEW - INSET,
                (width - 2.0 * INSET).max(0.0),
                OVERVIEW,
            )
        }

        pub fn x_of(&self, frame: f64) -> f64 {
            let (v0, v1) = self.view.get();
            (frame - v0) / (v1 - v0) * self.obj().width() as f64
        }

        pub fn frame_at(&self, x: f64) -> f64 {
            let (v0, v1) = self.view.get();
            v0 + x / self.obj().width().max(1) as f64 * (v1 - v0)
        }

        fn overview_x(&self, frame: f64) -> f64 {
            let strip = self.overview();
            strip.x() as f64 + frame / self.frames.get().max(1.0) * strip.width() as f64
        }

        fn overview_frame(&self, x: f64) -> f64 {
            let strip = self.overview();
            (x - strip.x() as f64) / strip.width().max(1.0) as f64 * self.frames.get()
        }

        /// What pressing at a point would take hold of.
        fn hit(&self, x: f64, y: f64) -> Drag {
            if y as f32 >= self.overview().y() - INSET / 2.0 {
                let (v0, v1) = self.view.get();
                let f = self.overview_frame(x);
                let grab = if (v0..=v1).contains(&f) { f - v0 } else { (v1 - v0) / 2.0 };
                return Drag::Overview { grab };
            }
            let (a, b) = self.selection.get();
            let (xa, xb) = (self.x_of(a), self.x_of(b));
            let (da, db) = ((x - xa).abs(), (x - xb).abs());
            if da <= GRAB && da <= db && b > a {
                Drag::Start
            } else if db <= GRAB {
                Drag::End
            } else {
                Drag::New { anchor: self.frame_at(x).clamp(0.0, self.frames.get()) }
            }
        }

        fn drag_begin(&self, x: f64, y: f64) {
            let hit = self.hit(x, y);
            self.drag.set(hit);
            if let Drag::Overview { .. } = hit {
                self.drag_to(x, 0.0);
            }
            self.update_cursor();
        }

        fn drag_to(&self, x: f64, moved: f64) {
            let obj = self.obj();
            let total = self.frames.get();
            let f = self.frame_at(x).clamp(0.0, total);
            let (a, b) = self.selection.get();
            match self.drag.get() {
                Drag::None => {}
                Drag::New { anchor } => {
                    if moved.abs() >= 3.0 {
                        obj.set_selection(anchor.min(f), anchor.max(f));
                    }
                }
                Drag::Start => {
                    if f > b {
                        self.drag.set(Drag::End);
                        obj.set_selection(b, f);
                    } else {
                        obj.set_selection(f, b);
                    }
                }
                Drag::End => {
                    if f < a {
                        self.drag.set(Drag::Start);
                        obj.set_selection(f, a);
                    } else {
                        obj.set_selection(a, f);
                    }
                }
                Drag::Overview { grab } => {
                    let (v0, v1) = self.view.get();
                    let start = self.overview_frame(x) - grab;
                    self.set_view(start, start + v1 - v0);
                }
            }
        }

        pub fn update_cursor(&self) {
            let (x, y) = self.pointer.get();
            let drag = match self.drag.get() {
                Drag::None => self.hit(x, y),
                drag => drag,
            };
            let name = match drag {
                Drag::Start | Drag::End => "col-resize",
                Drag::Overview { .. } => "pointer",
                Drag::New { .. } | Drag::None => "text",
            };
            self.obj().set_cursor_from_name(Some(name));
        }

        /// Shows frames `start..end`, kept within the capture.
        pub fn set_view(&self, start: f64, end: f64) {
            let total = self.frames.get();
            let min = (MIN_VIEW * self.rate.get()).min(total);
            let length = (end - start).clamp(min, total);
            let start = start.clamp(0.0, total - length);
            self.view.set((start, start + length));
            self.obj().queue_draw();
        }

        /// The summary of each pixel column between `x0` and `x1` of frames
        /// `f0..f1`.
        fn columns(&self, x0: f32, x1: f32, f0: f64, f1: f64) -> Vec<(f32, Bin)> {
            let peaks = self.peaks.borrow();
            let Some(peaks) = peaks.as_ref() else {
                return Vec::new();
            };
            let samples = self.samples.borrow();
            let width = (x1 - x0).max(0.0);
            let per_px = (f1 - f0) / width as f64;
            (0..width.ceil() as usize)
                .map(|i| {
                    let from = (f0 + i as f64 * per_px).floor().max(0.0) as usize;
                    let to = ((f0 + (i + 1) as f64 * per_px).ceil() as usize).max(from + 1);
                    (x0 + i as f32 + 0.5, peaks.range(&samples, from, to))
                })
                .collect()
        }
    }

    impl WidgetImpl for EditWave {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            if self.peaks.borrow().is_none() {
                return;
            }
            let (width, height) = (obj.width() as f32, obj.height() as f32);
            let area = self.wave_area();
            let fg = obj.color();
            let colour = self.colour.get().unwrap_or_else(|| draw::audio_colour(None));
            let neutral = (draw::alpha(&fg, 0.22), draw::alpha(&fg, 0.38));
            let lit = (draw::alpha(&colour, 0.5), colour);
            let (v0, v1) = self.view.get();
            let (a, b) = self.selection.get();
            let (xa, xb) = (self.x_of(a) as f32, self.x_of(b) as f32);
            let rate = self.rate.get();

            snapshot.push_rounded_clip(&gsk::RoundedRect::from_rect(
                graphene::Rect::new(0.0, 0.0, width, height),
                12.0,
            ));

            // The selection, lit up; the rest of the audio greyed.
            let chosen = graphene::Rect::new(xa, area.y(), xb - xa, area.height());
            snapshot.append_color(&draw::alpha(&colour, 0.13), &chosen);
            snapshot.append_color(
                &draw::alpha(&fg, 0.08),
                &graphene::Rect::new(0.0, area.height() / 2.0 - 0.5, width, 1.0),
            );
            let columns = self.columns(0.0, width, v0, v1);
            draw::waveform(snapshot, &area, &columns, &neutral.0, &neutral.1);
            snapshot.push_clip(&chosen);
            draw::waveform(snapshot, &area, &columns, &lit.0, &lit.1);
            snapshot.pop();

            // Edges, each with a tab to take hold of.
            for x in [xa, xb] {
                draw::vline(snapshot, &area, x, 2.0, &colour);
                let tab = graphene::Rect::new(x - 5.0, area.y() + 6.0, 10.0, 22.0);
                snapshot.push_rounded_clip(&gsk::RoundedRect::from_rect(tab, 5.0));
                snapshot.append_color(&colour, &tab);
                snapshot.pop();
            }

            if let Some(at) = self.playhead.get() {
                draw::vline(snapshot, &area, self.x_of(at) as f32, 2.0, &fg);
            }

            // Time from the start of the capture.
            let px_per_second = width as f64 / ((v1 - v0) / rate);
            let step = draw::tick_step(px_per_second, 120.0);
            let dim = draw::alpha(&fg, 0.55);
            let mut t = (v0 / rate / step).ceil() * step;
            while t <= v1 / rate {
                let x = self.x_of(t * rate) as f32;
                snapshot.append_color(
                    &draw::alpha(&fg, 0.15),
                    &graphene::Rect::new(x - 0.5, area.height(), 1.0, 5.0),
                );
                // Labels that would run off either edge are left out.
                if x >= 40.0 && x <= width - 40.0 {
                    draw::label(&*obj, snapshot, &draw::clock(t, step), x, area.height() + 5.0, 0.5, &dim);
                }
                t += step;
            }

            // All of the capture, with what the view shows outlined.
            let strip = self.overview();
            let total = self.frames.get();
            snapshot.push_rounded_clip(&gsk::RoundedRect::from_rect(strip, 8.0));
            snapshot.append_color(&draw::alpha(&fg, 0.05), &strip);
            let overview = self.columns(strip.x(), strip.x() + strip.width(), 0.0, total);
            let (oa, ob) = (self.overview_x(a) as f32, self.overview_x(b) as f32);
            draw::waveform(snapshot, &strip, &overview, &neutral.0, &neutral.1);
            let chosen = graphene::Rect::new(oa, strip.y(), (ob - oa).max(1.0), strip.height());
            snapshot.push_clip(&chosen);
            draw::waveform(snapshot, &strip, &overview, &lit.0, &lit.1);
            snapshot.pop();
            if let Some(at) = self.playhead.get() {
                draw::vline(snapshot, &strip, self.overview_x(at) as f32, 1.0, &fg);
            }
            let (w0, w1) = (self.overview_x(v0) as f32, self.overview_x(v1) as f32);
            if v1 - v0 < total {
                let outside = draw::alpha(&fg, 0.12);
                snapshot.append_color(&outside, &graphene::Rect::new(strip.x(), strip.y(), w0 - strip.x(), strip.height()));
                snapshot.append_color(&outside, &graphene::Rect::new(w1, strip.y(), strip.x() + strip.width() - w1, strip.height()));
                let window = gsk::RoundedRect::from_rect(
                    graphene::Rect::new(w0, strip.y(), (w1 - w0).max(4.0), strip.height()),
                    8.0,
                );
                let edge = draw::alpha(&fg, 0.6);
                snapshot.append_border(&window, &[1.5; 4], &[edge, edge, edge, edge]);
            }
            snapshot.pop();
            snapshot.pop();
        }
    }
}

glib::wrapper! {
    pub struct EditWave(ObjectSubclass<imp::EditWave>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl EditWave {
    pub fn new(colour: Option<gdk::RGBA>) -> Self {
        let wave: Self = glib::Object::new();
        wave.imp().colour.set(colour);
        wave
    }

    /// Shows a capture, all of it, with `selection` (in frames) chosen.
    pub fn set_audio(&self, samples: Rc<Vec<f32>>, rate: u32, channels: u16, selection: (f64, f64)) {
        let imp = self.imp();
        let channels = channels.max(1) as usize;
        let frames = (samples.len() / channels) as f64;
        imp.peaks.replace(Some(Peaks::new(&samples, channels as u16, 64)));
        imp.samples.replace(samples);
        imp.channels.set(channels);
        imp.rate.set(rate as f64);
        imp.frames.set(frames);
        imp.playhead.set(None);
        imp.view.set((0.0, frames));
        self.set_selection(selection.0, selection.1);
    }

    pub fn samples(&self) -> Rc<Vec<f32>> {
        self.imp().samples.borrow().clone()
    }

    pub fn rate(&self) -> f64 {
        self.imp().rate.get()
    }

    /// The chosen frames.
    pub fn selection(&self) -> (usize, usize) {
        let (a, b) = self.imp().selection.get();
        (a.round() as usize, b.round() as usize)
    }

    pub fn set_selection(&self, start: f64, end: f64) {
        let total = self.imp().frames.get();
        self.imp()
            .selection
            .set((start.clamp(0.0, total), end.clamp(0.0, total)));
        self.queue_draw();
        for changed in self.imp().changed.borrow().iter() {
            changed();
        }
    }

    pub fn connect_selection_changed(&self, f: impl Fn() + 'static) {
        self.imp().changed.borrow_mut().push(Box::new(f));
    }

    pub fn set_playhead(&self, frame: Option<f64>) {
        if self.imp().playhead.replace(frame) != frame {
            self.queue_draw();
        }
    }

    /// Zooms by `factor` (above 1 zooms out) keeping what is at `x` in place.
    pub fn zoom(&self, factor: f64, x: f64) {
        let imp = self.imp();
        let (v0, v1) = imp.view.get();
        let at = imp.frame_at(x);
        let length = (v1 - v0) * factor;
        let ratio = (at - v0) / (v1 - v0);
        imp.set_view(at - ratio * length, at - ratio * length + length);
    }

    pub fn zoom_fit(&self) {
        self.imp().set_view(0.0, self.imp().frames.get());
    }

    /// Fills the view with the selection and a little either side.
    pub fn zoom_selection(&self) {
        let (a, b) = self.imp().selection.get();
        let margin = ((b - a) * 0.08).max(self.rate() * 0.05);
        self.imp().set_view(a - margin, b + margin);
    }
}
