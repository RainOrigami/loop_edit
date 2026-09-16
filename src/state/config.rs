use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub audio: AudioConfig,
    pub buffer: BufferConfig,
    pub output: OutputConfig,
    pub ui: UiConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    pub capture_device: String,
    pub playback_device: String,
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BufferConfig {
    pub duration_seconds: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    pub default_folder: String,
    pub filename_pattern: String,
    pub bit_depth: u16,
    #[serde(default)]
    pub gain_db: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    pub waveform_color: [u8; 4]
}

impl Config {
    /// Load configuration from file, creating default if missing
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;

        if !config_path.exists() {
            log::info!("Config file not found, creating default at {:?}", config_path);
            Self::create_default_config(&config_path)?;
        }

        let config_str = fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read config file: {:?}", config_path))?;

        let config: Config = toml::from_str(&config_str)
            .with_context(|| "Failed to parse config file")?;

        log::info!("Loaded config from {:?}", config_path);
        Ok(config)
    }

    /// Get the path to the config file
    pub fn config_path() -> Result<PathBuf> {
        let proj_dirs = ProjectDirs::from("", "", "loop_edit")
            .context("Could not determine config directory")?;

        let config_dir = proj_dirs.config_dir();
        fs::create_dir_all(config_dir)
            .with_context(|| format!("Failed to create config directory: {:?}", config_dir))?;

        Ok(config_dir.join("config.toml"))
    }

    /// Create default config file
    fn create_default_config(path: &PathBuf) -> Result<()> {
        let default_config = include_str!("../../config/default_config.toml");
        fs::write(path, default_config)
            .with_context(|| format!("Failed to write default config to {:?}", path))?;
        log::info!("Created default config file at {:?}", path);
        Ok(())
    }
}
