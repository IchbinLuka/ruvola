use color_eyre::Result;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, io::Write};

#[derive(Deserialize, Serialize, Debug, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub validation: ValidationConfig,
    #[serde(default)]
    pub deck_config: DeckConfig,
    #[serde(default)]
    pub special_letters: SpecialLetters,
}

impl AppConfig {
    pub fn load_from_file() -> Result<Self> {
        let config_path = get_system_config_dir()?;
        let config_file = format!("{}/vocab_trainer/config.toml", config_path);
        if !std::fs::exists(&config_file)? {
            let default_config = AppConfig::default();
            default_config.save_to_file()?;
            return Ok(default_config);
        }

        let config: AppConfig = toml::de::from_str(&std::fs::read_to_string(config_file)?)?;
        Ok(config)
    }

    pub fn save_to_file(&self) -> Result<()> {
        let config_path = get_system_config_dir()?;
        let config_file = format!("{}/vocab_trainer/config.toml", config_path);
        std::fs::create_dir_all(format!("{}/vocab_trainer", config_path))?;
        let mut file = std::fs::File::create(config_file)?;
        let serialized = toml::ser::to_string(self)?;
        file.write_all(serialized.as_bytes())?;
        Ok(())
    }
}

#[derive(Deserialize, Serialize, Debug)]
pub struct ValidationConfig {
    pub error_tolerance: usize,
    pub tolerance_min_length: usize,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            error_tolerance: 2,
            tolerance_min_length: 5,
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Default)]
pub struct SpecialLetters(pub HashMap<String, Vec<SpecialLettersConfig>>);

#[derive(Deserialize, Serialize, Debug)]
pub struct SpecialLettersConfig {
    pub base: String,
    pub special: Vec<String>,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct DeckConfig {
    pub deck_durations: Vec<u32>,
}

impl Default for DeckConfig {
    fn default() -> Self {
        Self {
            deck_durations: vec![1, 7, 14, 30, 60, 90, 180, 365],
        }
    }
}

#[cfg(target_os = "linux")]
fn get_system_config_dir() -> Result<String> {
    let config_dir = std::env::var("XDG_CONFIG_HOME")
        .or_else(|_| std::env::var("HOME").map(|home| format!("{}/.config", home)))?;
    Ok(config_dir)
}

#[cfg(target_os = "windows")]
fn get_system_config_dir() -> Result<String> {
    let config_dir = std::env::var("APPDATA")?;
    Ok(config_dir)
}

#[cfg(target_os = "macos")]
fn get_system_config_dir() -> Result<String> {
    let config_dir =
        std::env::var("HOME").map(|home| format!("{}/Library/Application Support", home))?;
    Ok(config_dir)
}