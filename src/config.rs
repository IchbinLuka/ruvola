use anyhow::Result;
use chrono::Duration;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Deserialize, Debug, Default, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct AppConfig {
    pub memorization: MemorizationConfig,
    pub validation: ValidationConfig,
    pub deck_config: DeckConfig,
    pub special_letters: SpecialLetters,
    pub keybindings: KeybindsConfig,
}

impl AppConfig {
    pub fn load_from_config_file(local_path: Option<&str>) -> Result<Self> {
        const LOCAL_CONFIG_FILE: &str = "./ruvola.toml";
        let local_config_path = local_path.unwrap_or(LOCAL_CONFIG_FILE);

        let config_path = get_system_config_dir()?;
        let config_file = format!("{}/ruvola/config.toml", config_path);
        if std::fs::exists(&config_file)? {
            let base_config = toml::de::from_str(&std::fs::read_to_string(&config_file)?)?;
            if std::fs::exists(local_config_path)? {
                let override_config =
                    toml::de::from_str(&std::fs::read_to_string(local_config_path)?)?;
                let merged_config = deep_override_config(base_config, override_config);
                Ok(merged_config.try_into()?)
            } else {
                Ok(base_config.try_into()?)
            }
        } else {
            Ok(Self::default())
        }
    }
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct KeybindsConfig {
    pub skip: char,
    pub accept_anyway: char,
    pub reject_anyway: char,
    pub force_quit: char,
    pub save_and_quit: char,
    pub edit_mode: char,
    pub help: char,
}

impl Default for KeybindsConfig {
    fn default() -> Self {
        Self {
            skip: 's',
            accept_anyway: 'a',
            reject_anyway: 'r',
            force_quit: 'Q',
            save_and_quit: 'w',
            edit_mode: 'i',
            help: 'h',
        }
    }
}

#[derive(Deserialize, Serialize, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
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
#[serde(default, deny_unknown_fields)]
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
#[serde(default, deny_unknown_fields)]
pub struct SpecialLetters(pub HashMap<String, Vec<SpecialLettersConfig>>);

#[derive(Deserialize, Serialize, Debug, PartialEq)]
pub struct SpecialLettersConfig {
    pub base: String,
    pub special: Vec<String>,
}

#[derive(Deserialize, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct DeckConfig {
    #[serde(alias = "deck_durations")]
    pub deck_intervals: Vec<DeckInverval>,
    pub change_deck_in_ignore_date: bool,
}

impl Default for DeckConfig {
    fn default() -> Self {
        Self {
            deck_intervals: [0, 1, 7, 14, 30, 60, 90, 180, 365]
                .iter()
                .map(|&days| DeckInverval(Duration::days(days)))
                .collect(),
            change_deck_in_ignore_date: false,
        }
    }
}

#[derive(Deserialize, Debug, PartialEq)]
#[serde(try_from = "DeckIntervalSer")]
pub struct DeckInverval(pub Duration);

#[derive(Deserialize, Serialize, Debug, PartialEq)]
#[serde(untagged)]
enum DeckIntervalSer {
    Days(u32),
    Complex(String),
}

#[derive(Debug)]
enum IntervalParseError {
    InvalidFormat,
    InvalidNumber,
    InvalidUnit,
    ExpectedDigit,
}
impl std::fmt::Display for IntervalParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IntervalParseError::InvalidFormat => write!(f, "Invalid format"),
            IntervalParseError::InvalidNumber => write!(f, "Invalid number"),
            IntervalParseError::InvalidUnit => write!(f, "Invalid unit"),
            IntervalParseError::ExpectedDigit => write!(f, "Expected digit"),
        }
    }
}
impl std::error::Error for IntervalParseError {}

impl TryFrom<DeckIntervalSer> for DeckInverval {
    type Error = IntervalParseError;

    fn try_from(value: DeckIntervalSer) -> std::result::Result<Self, Self::Error> {
        match value {
            DeckIntervalSer::Days(days) => Ok(DeckInverval(Duration::days(days as i64))),
            DeckIntervalSer::Complex(complex) => {
                let duration = parse_complex_duration(&complex)?;
                Ok(DeckInverval(duration))
            }
        }
    }
}

fn parse_complex_duration(complex: &str) -> Result<Duration, IntervalParseError> {
    let mut current_duration = Duration::zero();
    let mut current_number = Vec::new();
    for c in complex.chars() {
        if c.is_ascii_digit() {
            current_number.push(c);
            continue;
        }
        if current_number.is_empty() {
            return Err(IntervalParseError::ExpectedDigit);
        }
        let total_number: u32 = current_number
            .iter()
            .collect::<String>()
            .parse()
            .map_err(|_| IntervalParseError::InvalidNumber)?;
        current_duration += match c {
            'd' => Duration::days(total_number as i64),
            'h' => Duration::hours(total_number as i64),
            'm' => Duration::minutes(total_number as i64),
            's' => Duration::seconds(total_number as i64),
            _ => return Err(IntervalParseError::InvalidUnit),
        };
        current_number.clear();
    }
    if current_number.is_empty() {
        Ok(current_duration)
    } else {
        Err(IntervalParseError::InvalidFormat)
    }
}

fn deep_override_config(base: toml::Value, override_config: toml::Value) -> toml::Value {
    match (base, override_config) {
        (toml::Value::Table(mut base_map), toml::Value::Table(override_map)) => {
            for (key, value) in override_map {
                if let Some(base_value) = base_map.get_mut(&key) {
                    *base_value = deep_override_config(base_value.clone(), value);
                } else {
                    base_map.insert(key, value);
                }
            }
            toml::Value::Table(base_map)
        }
        (_, override_value) => override_value,
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

    use toml::toml;

    use super::*;

    #[test]
    fn deep_override_config_test() {
        let base: toml::Value = toml! {
            [section]
            key = "value"
            nested = { key1 = "value1", key2 = "value2" }
        }
        .into();
        let override_config: toml::Value = toml! {
            [section]
            key = "new_value"
            nested = { key3 = "new_value2" }

            [new_section]
            key = "new_value"
        }
        .into();
        let expected: toml::Value = toml! {
            [section]
            key = "new_value"
            nested = { key1 = "value1", key2 = "value2", key3 = "new_value2" }

            [new_section]
            key = "new_value"
        }
        .into();

        let result = deep_override_config(base, override_config);
        assert_eq!(result, expected);
    }

    #[test]
    fn validate_config_preset() {
        let config: AppConfig =
            toml::de::from_str(&std::fs::read_to_string("config_preset/config.toml").unwrap())
                .unwrap();
        assert_eq!(config.memorization.do_memorization_round, true);
        assert_eq!(config.memorization.memorization_reversed, false);
        assert_eq!(config.validation.error_tolerance, 2);
        assert_eq!(config.validation.tolerance_min_length, 5);
        assert_eq!(config.special_letters.0.len(), 3);
    }

    #[test]
    fn system_config_dir() {
        assert!(fs::exists(get_system_config_dir().unwrap()).unwrap());
        assert!(
            fs::metadata(get_system_config_dir().unwrap())
                .unwrap()
                .is_dir()
        );
    }

    #[test]
    fn parse_complex_duration_test() {
        let duration = parse_complex_duration("10d21h3m4s").unwrap();
        assert_eq!(
            duration,
            Duration::days(10) + Duration::hours(21) + Duration::minutes(3) + Duration::seconds(4)
        );
        let duration = parse_complex_duration("1d").unwrap();
        assert_eq!(duration, Duration::days(1));
        let duration = parse_complex_duration("2h").unwrap();
        assert_eq!(duration, Duration::hours(2));
        let duration = parse_complex_duration("3m").unwrap();
        assert_eq!(duration, Duration::minutes(3));
        let duration = parse_complex_duration("4s").unwrap();
        assert_eq!(duration, Duration::seconds(4));
        let duration = parse_complex_duration("").unwrap();
        assert_eq!(duration, Duration::zero());

        let invalid = parse_complex_duration("1dhm4s");
        assert!(invalid.is_err());
        let invalid = parse_complex_duration("1d2h3m4");
        assert!(invalid.is_err());
        let invalid = parse_complex_duration("1d2h3m4x");
        assert!(invalid.is_err());
    }
}
