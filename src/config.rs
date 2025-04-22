use color_eyre::Result;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, io::Write};

#[derive(Deserialize, Serialize, Debug)]
pub struct AppConfig {
    pub deck_config: DeckConfig,
    pub special_letters: SpecialLetters,
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            deck_config: DeckConfig {
                deck_durations: vec![1, 7, 14, 30, 60, 90, 180, 365],
            },
            special_letters: SpecialLetters(HashMap::new()),
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Read;

    #[test]
    fn test_app_config() {
        let mut file = File::open("test.toml").unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();

        let config: AppConfig = toml::from_str(&contents).unwrap();
        println!("{:?}", config);
        assert_eq!(config.deck_config.deck_durations.len(), 3);
        assert_eq!(config.special_letters.0.len(), 2);
    }
}
