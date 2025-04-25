use std::{error::Error, io::BufRead};

use chrono::NaiveDateTime;

#[derive(Debug)]
pub struct Vocab {
    pub word_a: String,
    pub word_b: String,
    pub due_date: Option<NaiveDateTime>,
    pub due_date_reverse: Option<NaiveDateTime>,
    pub deck: Option<u8>,
    pub deck_reverse: Option<u8>,
}

impl Vocab {
    pub fn update_metadata(&mut self, deck: u8, due_date: NaiveDateTime, reverse: bool) {
        if reverse {
            self.deck_reverse = Some(deck);
            self.due_date_reverse = Some(due_date);
        } else {
            self.deck = Some(deck);
            self.due_date = Some(due_date);
        }
    }

    pub fn get_deck(&self, reverse: bool) -> Option<u8> {
        if reverse {
            self.deck_reverse
        } else {
            self.deck
        }
    }

    fn from_line(line: &str) -> Result<Vocab, VocaLineError> {
        use VocaLineError::*;

        let mut parts = line.split('\t');
        let word_a = parts.next().ok_or(MissingWordA)?.to_string();
        let word_b = parts.next().ok_or(MissingWordB)?.to_string();
        let (deck, due_date, deck_b, due_date_b) = match parts.next() {
            Some(deck) => {
                let deck = deck.parse::<u8>().map_err(|_| InvalidDeck)?;
                let date_str = parts.next().ok_or(MissingDueDate)?;
                let date = NaiveDateTime::parse_from_str(date_str, "%Y-%m-%d %H:%M:%S")
                    .map_err(|_| InvalidDueDate)?;
                let deck_b = parts
                    .next()
                    .ok_or(MissingDeck)?
                    .parse::<u8>()
                    .map_err(|_| InvalidDeck)?;
                let date_b = NaiveDateTime::parse_from_str(
                    parts.next().ok_or(MissingDueDate)?,
                    "%Y-%m-%d %H:%M:%S",
                )
                .map_err(|_| InvalidDueDate)?;
                (Some(deck), Some(date), Some(deck_b), Some(date_b))
            }
            None => (None, None, None, None),
        };

        Ok(Vocab {
            word_a,
            word_b,
            due_date,
            deck,
            due_date_reverse: due_date_b,
            deck_reverse: deck_b,
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

#[derive(Debug)]
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
    fn test_voca_card() {
        let line = "hello\tworld\t1\t2023-10-01 12:00:00\t2\t2024-10-01 13:00:00";
        let card = Vocab::from_line(line).unwrap();
        assert_eq!(card.word_a, "hello");
        assert_eq!(card.word_b, "world");
        assert_eq!(card.deck, Some(1));
        assert_eq!(
            card.due_date,
            Some(
                NaiveDateTime::parse_from_str("2023-10-01 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap()
            )
        );
        assert_eq!(card.deck_reverse, Some(2));
        assert_eq!(
            card.due_date_reverse,
            Some(
                NaiveDateTime::parse_from_str("2024-10-01 13:00:00", "%Y-%m-%d %H:%M:%S").unwrap()
            )
        );
    }
}
