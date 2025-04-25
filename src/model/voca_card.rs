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

    fn from_line(line: &str) -> Result<Vocab, std::io::Error> {
        let mut parts = line.split('\t');
        let word_a = parts
            .next()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "Missing word_a"))?
            .to_string();
        let word_b = parts
            .next()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "Missing word_b"))?
            .to_string();
        let (deck, due_date, deck_b, due_date_b) = match parts.next() {
            Some(deck) => {
                let deck = deck.parse::<u8>().ok();
                let date_str = parts
                    .next()
                    .ok_or_else(|| {
                        std::io::Error::new(std::io::ErrorKind::InvalidData, "Missing due_date")
                    })?
                    .trim();
                let date = NaiveDateTime::parse_from_str(date_str, "%Y-%m-%d %H:%M:%S")
                    .expect("Failed to parse date");
                let deck_b = parts.next().and_then(|d| d.parse::<u8>().ok());
                let date_b = parts.next().map(|d| {
                    NaiveDateTime::parse_from_str(d, "%Y-%m-%d %H:%M:%S")
                        .expect("Failed to parse date")
                });
                (deck, Some(date), deck_b, date_b)
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
pub struct VocaCardDataset {
    pub cards: Vec<Vocab>,
    pub file_path: String,
    pub lang_a: String,
    pub lang_b: String,
}

#[derive(Debug)]
pub enum VocaParseError {
    EmptyFile,
    IoError(std::io::Error),
    InvalidFormat,
}

impl std::fmt::Display for VocaParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VocaParseError::EmptyFile => write!(f, "The file is empty"),
            VocaParseError::IoError(err) => write!(f, "IO error: {}", err),
            VocaParseError::InvalidFormat => write!(f, "Invalid format"),
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
        let header = lines.next().ok_or(VocaParseError::EmptyFile)??;
        let mut parts = header.split('\t');
        let lang_a = parts
            .next()
            .ok_or(VocaParseError::InvalidFormat)?
            .to_string();
        let lang_b = parts
            .next()
            .ok_or(VocaParseError::InvalidFormat)?
            .to_string();
        for line in lines {
            let line = line?;
            if !line.trim().is_empty() {
                let card = Vocab::from_line(&line)?;
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
