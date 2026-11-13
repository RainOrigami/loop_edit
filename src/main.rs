use adw::prelude::*;
use gtk::glib;

mod audio;
mod state;
mod ui;
mod util;

const APP_ID: &str = "net.codefreak.LoopEdit";

fn main() -> glib::ExitCode {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    // One instance: launching again brings the window back rather than
    // starting a second recording.
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_startup(|_| {
        let css = gtk::CssProvider::new();
        css.load_from_string(ui::CSS);
        gtk::style_context_add_provider_for_display(
            &gtk::gdk::Display::default().expect("a display"),
            &css,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    });
    // `--screenshot <file.png> [--capture]`: save a picture of the window,
    // on the editing page with `--capture`, and quit.
    let args: Vec<String> = std::env::args().collect();
    let shot = args
        .iter()
        .position(|a| a == "--screenshot")
        .and_then(|i| args.get(i + 1))
        .map(std::path::PathBuf::from);
    let capture = args.iter().any(|a| a == "--capture");
    app.connect_activate(move |app| match app.active_window() {
        Some(window) => window.present(),
        None => {
            let window = ui::build(app);
            window.present();
            if let Some(path) = shot.clone() {
                screenshot(app, &window, path, capture);
            }
        }
    });
    app.run_with_args::<&str>(&[])
}

fn screenshot(app: &adw::Application, window: &adw::ApplicationWindow, path: std::path::PathBuf, capture: bool) {
    let window = window.clone();
    let app = app.clone();
    glib::timeout_add_local_once(std::time::Duration::from_secs(3), move || {
        if capture {
            WidgetExt::activate_action(&window, "win.capture", None).ok();
        }
        glib::timeout_add_local_once(std::time::Duration::from_millis(700), move || {
            let (width, height) = (window.width() as f64, window.height() as f64);
            let picture = gtk::Snapshot::new();
            gtk::WidgetPaintable::new(Some(&window)).snapshot(&picture, width, height);
            let saved = match (picture.to_node(), window.native().and_then(|n| n.renderer())) {
                (Some(node), Some(renderer)) => renderer
                    .render_texture(&node, None)
                    .save_to_png(&path)
                    .map_err(|e| e.to_string()),
                _ => Err("nothing to draw".to_owned()),
            };
            if let Err(e) = saved {
                eprintln!("loop_edit: {}: {e}", path.display());
            }
            app.quit();
        });
    });
}
