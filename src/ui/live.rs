//! The waveform of the audio being recorded, newest at the right.

use std::cell::{Cell, RefCell};
use std::sync::Arc;

use gtk::{gdk, glib, graphene, gsk, prelude::*, subclass::prelude::*};
use parking_lot::RwLock;

use super::draw;
use crate::audio::peaks::{Bin, LivePeaks};
use crate::audio::AudioRingBuffer;

/// Bins per second of the live waveform.
const BINS_PER_SECOND: u32 = 100;
/// Room under the waveform for the time labels.
const RULER: f32 = 22.0;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct LiveWave {
        pub ring: RefCell<Option<Arc<RwLock<AudioRingBuffer>>>>,
        pub peaks: RefCell<Option<LivePeaks>>,
        /// `total_written` at the last read.
        pub read: Cell<u64>,
        pub seconds: Cell<f64>,
        pub colour: Cell<Option<gdk::RGBA>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LiveWave {
        const NAME: &'static str = "LoopEditLiveWave";
        type Type = super::LiveWave;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("wave");
        }
    }

    impl ObjectImpl for LiveWave {
        fn constructed(&self) {
            self.parent_constructed();
            // Runs every frame while the widget is on screen, focused or not.
            self.obj().add_tick_callback(|wave, _| {
                wave.imp().pull();
                wave.queue_draw();
                glib::ControlFlow::Continue
            });
        }
    }

    impl LiveWave {
        /// Summarises what was recorded since the last frame.
        fn pull(&self) {
            let ring = self.ring.borrow();
            let mut peaks = self.peaks.borrow_mut();
            let (Some(ring), Some(peaks)) = (ring.as_ref(), peaks.as_mut()) else {
                return;
            };
            // Copy, then let go of the lock before the work.
            let (new, read) = ring.read().read_since(self.read.get());
            self.read.set(read);
            peaks.push(&new);
        }
    }

    impl WidgetImpl for LiveWave {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            let peaks = self.peaks.borrow();
            let Some(peaks) = peaks.as_ref() else {
                return;
            };
            let (width, height) = (obj.width() as f32, obj.height() as f32);
            let area = graphene::Rect::new(0.0, 0.0, width, (height - RULER).max(0.0));
            let fg = obj.color();
            let colour = self
                .colour
                .get()
                .unwrap_or_else(|| draw::audio_colour(None));

            snapshot.push_rounded_clip(&gsk::RoundedRect::from_rect(
                graphene::Rect::new(0.0, 0.0, width, height),
                12.0,
            ));
            snapshot.append_color(
                &draw::alpha(&fg, 0.08),
                &graphene::Rect::new(0.0, area.height() / 2.0 - 0.5, width, 1.0),
            );

            // Columns sit on a grid fixed to the audio rather than to the
            // widget, and the whole picture slides by fractions of a column,
            // so the waveform scrolls smoothly instead of shimmering.
            let bins = peaks.bins();
            let n = bins.len() as i64;
            let made = peaks.made() as i64;
            let per_column = ((n as f32 / width).ceil() as i64).max(1);
            let px_per_bin = width / n as f32;
            let now = made as f32 + peaks.progress();
            let x_of = |bin: f32| width - (now - bin) * px_per_bin;
            let oldest = made - n;
            let mut columns = Vec::new();
            let mut column = oldest.div_euclid(per_column);
            while column * per_column < made {
                let from = (column * per_column).max(oldest);
                let to = ((column + 1) * per_column).min(made);
                let bin = Bin::merge((from..to).map(|i| bins[(i - oldest) as usize]));
                columns.push((x_of((from + to) as f32 / 2.0), bin));
                column += 1;
            }
            draw::waveform(
                snapshot,
                &area,
                &columns,
                &draw::alpha(&colour, 0.45),
                &colour,
            );

            // Seconds ago; these stay put while the audio moves past them.
            let seconds = self.seconds.get();
            let px_per_second = width as f64 / seconds;
            let step = draw::tick_step(px_per_second, 110.0).max(1.0);
            let dim = draw::alpha(&fg, 0.55);
            let grid = draw::alpha(&fg, 0.06);
            let mut ago = step;
            draw::label(&*obj, snapshot, "now", width - 10.0, area.height() + 3.0, 1.0, &dim);
            while ago <= seconds + 1e-9 {
                let x = width - (ago * px_per_second) as f32;
                snapshot.append_color(&grid, &graphene::Rect::new(x - 0.5, 0.0, 1.0, area.height()));
                // Labels that would run off the left edge are left out.
                if x >= 30.0 {
                    let text = if ago < 60.0 {
                        format!("−{ago}s")
                    } else {
                        format!("−{}", draw::clock(ago, step))
                    };
                    draw::label(&*obj, snapshot, &text, x, area.height() + 3.0, 0.5, &dim);
                }
                ago += step;
            }
            snapshot.pop();
        }
    }
}

glib::wrapper! {
    pub struct LiveWave(ObjectSubclass<imp::LiveWave>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl LiveWave {
    pub fn new(ring: Arc<RwLock<AudioRingBuffer>>, colour: Option<gdk::RGBA>) -> Self {
        let wave: Self = glib::Object::new();
        let imp = wave.imp();
        let (rate, channels, seconds) = {
            let ring = ring.read();
            (ring.sample_rate(), ring.channels(), ring.duration_secs() as f64)
        };
        let capacity = (seconds * BINS_PER_SECOND as f64).round() as usize;
        imp.peaks.replace(Some(LivePeaks::new(
            capacity.max(1),
            (rate / BINS_PER_SECOND).max(1) as usize,
            channels,
        )));
        imp.seconds.set(seconds);
        imp.colour.set(colour);
        imp.ring.replace(Some(ring));
        wave
    }
}
