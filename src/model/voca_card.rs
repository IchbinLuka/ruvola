use std::{error::Error, io::BufRead, sync::LazyLock};

use chrono::{DateTime, NaiveDateTime};

use crate::FilterMode;

#[derive(Debug, Clone)]
pub struct Vocab {
    pub word_a: VocabWord,
    pub word_b: VocabWord,
    pub metadata: Option<VocabMetadata>,
}

#[derive(Debug, Clone)]
pub struct VocabWord {
    pub base: String,
    pub variants: Vec<String>,
}

impl VocabWord {
    pub fn from_str(s: &str) -> Self {
        static BRACKET_REGEX: LazyLock<regex::Regex> = LazyLock::new(|| {
            regex::Regex::new(r"\(.*\)").expect("Failed to compile bracket regex")
        });

        let base = s.to_string();
        let mut variants = vec![base.clone()];
        let comma_split = s.split(',').collect::<Vec<&str>>();
        // If we have only one part, base does not contain a comma, so don't do anything
        if comma_split.len() > 1 {
            variants.extend(
                comma_split
                    .iter()
                    .map(|v| v.trim().to_string())
                    .collect::<Vec<String>>(),
            );
        }

        let bracket_variants = variants
            .iter()
            .filter_map(|s| {
                BRACKET_REGEX
                    .find(s)
                    .map(|_| BRACKET_REGEX.replace_all(s, "").trim().to_string())
            })
            .collect::<Vec<String>>();
        variants.extend(bracket_variants);

        Self { base, variants }
    }
}

#[derive(Debug, Clone)]
pub struct VocabMetadata {
    pub due_date: NaiveDateTime,
    pub deck: u8,
    pub due_date_reverse: NaiveDateTime,
    pub deck_reverse: u8,
}

impl Default for VocabMetadata {
    fn default() -> Self {
        VocabMetadata {
            due_date: DateTime::UNIX_EPOCH.naive_utc(),
            deck: 0,
            due_date_reverse: DateTime::UNIX_EPOCH.naive_utc(),
            deck_reverse: 0,
        }
    }
}

impl Vocab {
    pub fn is_due(
        &self,
        reverse: bool,
        filter_mode: FilterMode,
        current_date: NaiveDateTime,
    ) -> bool {
        match filter_mode {
            FilterMode::All => true,
            FilterMode::Unseen => self.metadata.is_none(),
            FilterMode::Seen | FilterMode::Normal => {
                if let Some(metadata) = &self.metadata {
                    if reverse {
                        metadata.due_date_reverse < current_date
                    } else {
                        metadata.due_date < current_date
                    }
                } else {
                    matches!(filter_mode, FilterMode::Normal)
                }
            }
        }
    }

    pub fn update_metadata(&mut self, deck: u8, due_date: NaiveDateTime, reverse: bool) {
        if reverse {
            self.metadata = Some(VocabMetadata {
                deck_reverse: deck,
                due_date_reverse: due_date,
                ..self.metadata.clone().unwrap_or_default()
            });
        } else {
            self.metadata = Some(VocabMetadata {
                deck,
                due_date,
                ..self.metadata.clone().unwrap_or_default()
            });
        }
    }

    pub fn get_deck(&self, reverse: bool) -> Option<u8> {
        self.metadata.as_ref().map(|metadata| {
            if reverse {
                metadata.deck_reverse
            } else {
                metadata.deck
            }
        })
    }

    fn from_line(line: &str) -> Result<Vocab, VocaLineError> {
        use VocaLineError as VE;

        let mut parts = line.split('\t');
        let word_a = parts.next().ok_or(VE::MissingWordA)?;
        let word_b = parts.next().ok_or(VE::MissingWordB)?;
        let metadata = match parts.next() {
            Some(deck) => {
                let deck = deck.parse::<u8>().map_err(|_| VE::InvalidDeck)?;
                let date_str = parts.next().ok_or(VE::MissingDueDate)?;
                let date = NaiveDateTime::parse_from_str(date_str, "%Y-%m-%d %H:%M:%S")
                    .map_err(|_| VE::InvalidDueDate)?;
                let deck_b = parts
                    .next()
                    .ok_or(VE::MissingDeck)?
                    .parse::<u8>()
                    .map_err(|_| VE::InvalidDeck)?;
                let date_b = NaiveDateTime::parse_from_str(
                    parts.next().ok_or(VE::MissingDueDate)?,
                    "%Y-%m-%d %H:%M:%S",
                )
                .map_err(|_| VE::InvalidDueDate)?;
                Some(VocabMetadata {
                    deck,
                    due_date: date,
                    deck_reverse: deck_b,
                    due_date_reverse: date_b,
                })
            }

            None => None,
        };

        Ok(Vocab {
            word_a: VocabWord::from_str(word_a),
            word_b: VocabWord::from_str(word_b),
            metadata,
        })
    }
}

#[derive(Debug)]
enum VocaLineError {
    MissingWordA,
    MissingWordB,
    MissingDeck,
    MissingDueDate,
    InvalidDueDate,
    InvalidDeck,
}

impl std::fmt::Display for VocaLineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VocaLineError::MissingWordA => write!(f, "Missing word A"),
            VocaLineError::MissingWordB => write!(f, "Missing word B"),
            VocaLineError::MissingDeck => write!(f, "Missing deck"),
            VocaLineError::MissingDueDate => write!(f, "Missing due date"),
            VocaLineError::InvalidDueDate => write!(f, "Invalid due date"),
            VocaLineError::InvalidDeck => write!(f, "Invalid deck"),
        }
    }
}
impl std::error::Error for VocaLineError {}

impl VocaLineError {
    fn to_parse_error(&self, filename: &str, line: usize) -> VocaParseError {
        VocaParseError::InvalidFormat {
            filename: filename.into(),
            line,
            reason: self.to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct VocaCardDataset {
    pub cards: Vec<Vocab>,
    pub file_path: String,
    pub lang_a: String,
    pub lang_b: String,
}

#[derive(Debug)]
pub enum VocaParseError {
    EmptyFile {
        filename: String,
    },
    IoError(std::io::Error),
    InvalidFormat {
        filename: String,
        line: usize,
        reason: String,
    },
}

impl std::fmt::Display for VocaParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VocaParseError::EmptyFile { filename } => write!(f, "Empty file: {}", filename),
            VocaParseError::IoError(err) => write!(f, "IO error: {}", err),
            VocaParseError::InvalidFormat {
                filename,
                line,
                reason,
            } => {
                write!(
                    f,
                    "Invalid format in file '{}', line {}: {}",
                    filename, line, reason
                )
            }
        }
    }
}

impl Error for VocaParseError {}

impl From<std::io::Error> for VocaParseError {
    fn from(err: std::io::Error) -> Self {
        VocaParseError::IoError(err)
    }
}

impl VocaCardDataset {
    pub fn from_file(file_path: &str) -> Result<Self, VocaParseError> {
        let file = std::fs::File::open(file_path)?;
        let reader = std::io::BufReader::new(file);
        let mut cards = Vec::new();
        let mut lines = reader.lines();
        let header = lines.next().ok_or(VocaParseError::EmptyFile {
            filename: file_path.into(),
        })??;
        let mut parts = header.split('\t');
        let lang_a = parts
            .next()
            .ok_or(VocaParseError::InvalidFormat {
                filename: file_path.into(),
                line: 1,
                reason: "Invalid Header".into(),
            })?
            .to_string();
        let lang_b = parts
            .next()
            .ok_or(VocaParseError::InvalidFormat {
                filename: file_path.into(),
                line: 1,
                reason: "Expected second column".into(),
            })?
            .to_string();
        for (i, line) in lines.enumerate() {
            let line = line?;
            if !line.trim().is_empty() {
                let card =
                    Vocab::from_line(&line).map_err(|e| e.to_parse_error(file_path, i + 2))?;
                cards.push(card);
            }
        }
        Ok(VocaCardDataset {
            cards,
            file_path: file_path.to_string(),
            lang_a,
            lang_b,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_card() {
        let line = "hello\tworld\t1\t2023-10-01 12:00:00\t2\t2024-10-01 13:00:00";
        let card = Vocab::from_line(line).unwrap();
        assert_eq!(card.word_a.base, "hello");
        assert_eq!(card.word_b.base, "world");
        assert_eq!(card.metadata.as_ref().unwrap().deck, 1);
        assert_eq!(
            card.metadata.as_ref().unwrap().due_date,
            NaiveDateTime::parse_from_str("2023-10-01 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap()
        );
        assert_eq!(card.metadata.as_ref().unwrap().deck_reverse, 2);
        assert_eq!(
            card.metadata.as_ref().unwrap().due_date_reverse,
            NaiveDateTime::parse_from_str("2024-10-01 13:00:00", "%Y-%m-%d %H:%M:%S").unwrap()
        );
    }

    #[test]
    fn parse_card_with_variants() {
        let line = "hello,hi\tworld,earth\t1\t2023-10-01 12:00:00\t2\t2024-10-01 13:00:00";
        let card = Vocab::from_line(line).unwrap();
        assert_eq!(card.word_a.base, "hello,hi");
        assert_eq!(card.word_b.base, "world,earth");
        assert_eq!(card.word_a.variants, vec!["hello,hi", "hello", "hi"]);
        assert_eq!(card.word_b.variants, vec!["world,earth", "world", "earth"]);

        let line =
            "hello (greeting)\tworld (planet)\t1\t2023-10-01 12:00:00\t2\t2024-10-01 13:00:00";
        let card = Vocab::from_line(line).unwrap();
        assert_eq!(card.word_a.base, "hello (greeting)");
        assert_eq!(card.word_b.base, "world (planet)");
        assert_eq!(card.word_a.variants, vec!["hello (greeting)", "hello"]);
        assert_eq!(card.word_b.variants, vec!["world (planet)", "world"]);
    }
}
