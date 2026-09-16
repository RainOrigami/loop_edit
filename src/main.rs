use anyhow::Result;

mod app;
mod audio;
mod state;
mod ui;
mod util;

use app::LoopEditApp;

fn main() -> Result<()> {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    eframe::run_native(
        "Loop Edit",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_title("Loop Edit")
            .with_inner_size([
                800 as f32,
                600 as f32,
            ]),
            ..Default::default()
        },
        Box::new(|cc| match LoopEditApp::new(cc) {
            Ok(app) => Ok(Box::new(app)),
            Err(e) => {
                log::error!("Failed to initialize application: {}", e);
                std::process::exit(1);
            }
        }),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {}", e))?;

    Ok(())
}
