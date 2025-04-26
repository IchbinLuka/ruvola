use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Deserialize, Serialize, Debug, Default, PartialEq)]
#[serde(default)]
pub struct AppConfig {
    pub memorization: MemorizationConfig,
    pub validation: ValidationConfig,
    pub deck_config: DeckConfig,
    pub special_letters: SpecialLetters,
}

impl AppConfig {
    pub fn load_from_config_file() -> Result<Self> {
        let config_path = get_system_config_dir()?;
        let config_file = format!("{}/ruvola/config.toml", config_path);
        if std::fs::exists(&config_file)? {
            Self::load_from_file(&config_file)
        } else {
            Ok(Self::default())
        }
    }

    pub fn load_from_file(file_path: &str) -> Result<Self> {
        Ok(toml::de::from_str(&std::fs::read_to_string(file_path)?)?)
    }
}

#[derive(Deserialize, Serialize, Debug, PartialEq)]
#[serde(default)]
pub struct MemorizationConfig {
    pub do_memorization_round: bool,
    pub memorization_reversed: bool,
}

impl Default for MemorizationConfig {
    fn default() -> Self {
        Self {
            do_memorization_round: true,
            memorization_reversed: false,
        }
    }
}

#[derive(Deserialize, Serialize, Debug, PartialEq)]
#[serde(default)]
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

#[derive(Deserialize, Serialize, Debug, Default, PartialEq)]
#[serde(default)]
pub struct SpecialLetters(pub HashMap<String, Vec<SpecialLettersConfig>>);

#[derive(Deserialize, Serialize, Debug, PartialEq)]
pub struct SpecialLettersConfig {
    pub base: String,
    pub special: Vec<String>,
}

#[derive(Deserialize, Serialize, Debug, PartialEq)]
#[serde(default)]
pub struct DeckConfig {
    pub deck_durations: Vec<u32>,
}

impl Default for DeckConfig {
    fn default() -> Self {
        Self {
            deck_durations: vec![0, 1, 7, 14, 30, 60, 90, 180, 365],
        }
    }
}

#[cfg(target_os = "linux")]
fn get_system_config_dir() -> Result<String, std::env::VarError> {
    let config_dir = std::env::var("XDG_CONFIG_HOME")
        .or_else(|_| std::env::var("HOME").map(|home| format!("{}/.config", home)))?;
    Ok(config_dir)
}

#[cfg(target_os = "windows")]
fn get_system_config_dir() -> Result<String, std::env::VarError> {
    let config_dir = std::env::var("APPDATA")?;
    Ok(config_dir)
}

#[cfg(target_os = "macos")]
fn get_system_config_dir() -> Result<String, std::env::VarError> {
    let config_dir =
        std::env::var("HOME").map(|home| format!("{}/Library/Application Support", home))?;
    Ok(config_dir)
}


#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn validate_config_preset() {
        let config = AppConfig::load_from_file("config_preset/config.toml").unwrap();
        assert_eq!(config.memorization.do_memorization_round, true);
        assert_eq!(config.memorization.memorization_reversed, false);
        assert_eq!(config.validation.error_tolerance, 2);
        assert_eq!(config.validation.tolerance_min_length, 5);
        assert_eq!(config.deck_config.deck_durations, vec![0, 1, 7, 14, 30, 60, 90, 180, 365]);
        assert_eq!(config.special_letters.0.len(), 3);
    }

    #[test]
    fn system_config_dir() {
        assert!(fs::exists(get_system_config_dir().unwrap()).unwrap());
        assert!(fs::metadata(get_system_config_dir().unwrap()).unwrap().is_dir());
    }
}