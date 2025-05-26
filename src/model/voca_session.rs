use std::collections::VecDeque;

use crate::{
    FilterMode,
    config::{DeckConfig, MemorizationConfig, ValidationConfig},
};

use super::voca_card::{VocaCardDataset, VocaParseError, Vocab, VocabMetadata};
use std::io::Write;

pub struct VocabTask<'a> {
    pub query: &'a str,
    pub answer: &'a str,
    pub answer_variants: &'a [String],
    pub show_answer: bool,
}

impl VocabTask<'_> {
    pub fn is_correct(&self, answer: &str, val_config: &ValidationConfig) -> bool {
        for variant in self.answer_variants {
            if variant.len() < val_config.tolerance_min_length {
                if answer == variant {
                    return true;
                }
            } else if edit_distance::edit_distance(variant, answer) <= val_config.error_tolerance {
                return true;
            }
        }
        false
    }
}

#[derive(Debug)]
struct VocabItem {
    dataset: usize,
    card: usize,
    reverse: bool,
    memorization_card: bool,
}

pub struct VocaSession {
    datasets: Vec<VocaCardDataset>,
    queue: VecDeque<VocabItem>,
    has_changes: bool,
    total_due: usize,
}

impl VocaSession {
    fn new(
        datasets: Vec<VocaCardDataset>,
        filter_mode: FilterMode,
        sorted: bool,
        limit: Option<usize>,
        memorization_config: &MemorizationConfig,
    ) -> Self {
        let mut queue_seen = VecDeque::new();
        let mut queue_reverse = VecDeque::new();
        let mut queue_unseen = VecDeque::new();
        // let mut queue_reverse = VecDeque::new();
        let current_date = chrono::Local::now().naive_utc();
        let mut num_cards = 0;
        let mut all_vocabs = datasets
            .iter()
            .enumerate()
            .flat_map(|(i, dataset)| {
                dataset
                    .cards
                    .iter()
                    .enumerate()
                    .map(move |(j, card)| ((i, j), card))
            })
            .collect::<Vec<_>>();
        if sorted {
            all_vocabs.sort_by(
                |(_, Vocab { metadata: a, .. }), (_, Vocab { metadata: b, .. })| {
                    if let Some(a) = a {
                        if let Some(b) = b {
                            a.due_date.cmp(&b.due_date)
                        } else {
                            std::cmp::Ordering::Greater
                        }
                    } else if b.is_some() {
                        std::cmp::Ordering::Less
                    } else {
                        std::cmp::Ordering::Equal
                    }
                },
            );
        }
        for ((i, j), card) in all_vocabs {
            if let Some(limit) = limit {
                if num_cards >= limit {
                    break;
                }
            }

            let add_to_queue = card.is_due(false, filter_mode, current_date);
            let add_to_queue_reverse = card.is_due(true, filter_mode, current_date);

            let card_used = add_to_queue || add_to_queue_reverse;

            if card.metadata.is_none() && memorization_config.do_memorization_round && card_used {
                queue_unseen.push_back(VocabItem {
                    dataset: i,
                    card: j,
                    reverse: memorization_config.memorization_reversed,
                    memorization_card: true,
                });
            }

            if add_to_queue {
                queue_seen.push_back(VocabItem {
                    dataset: i,
                    card: j,
                    reverse: false,
                    memorization_card: false,
                });
            }

            if add_to_queue_reverse {
                queue_reverse.push_back(VocabItem {
                    dataset: i,
                    card: j,
                    reverse: true,
                    memorization_card: false,
                });
            }
            if card_used {
                num_cards += 1;
            }
        }

        for item in queue_seen {
            queue_unseen.push_back(item);
        }
        for item in queue_reverse {
            queue_unseen.push_back(item);
        }
        let total_due = queue_unseen.len();
        VocaSession {
            datasets,
            queue: queue_unseen,
            has_changes: false,
            total_due,
        }
    }

    #[inline(always)]
    pub fn has_changes(&self) -> bool {
        self.has_changes
    }

    pub fn current_task(&self) -> Option<VocabTask> {
        self.queue.front().and_then(|index| {
            self.datasets
                .get(index.dataset)
                .and_then(|d| d.cards.get(index.card))
                .map(|card| {
                    let query = if index.reverse {
                        &card.word_b
                    } else {
                        &card.word_a
                    };
                    let answer = if index.reverse {
                        &card.word_a
                    } else {
                        &card.word_b
                    };
                    VocabTask {
                        query: &query.base,
                        answer: &answer.base,
                        answer_variants: &answer.variants,
                        show_answer: index.memorization_card,
                    }
                })
        })
    }

    pub fn current_target_lang(&self) -> Option<&str> {
        self.queue.front().and_then(|index| {
            self.datasets.get(index.dataset).map(|d| {
                if index.reverse {
                    d.lang_a.as_ref()
                } else {
                    d.lang_b.as_ref()
                }
            })
        })
    }

    pub fn skip_card(&mut self) {
        if let Some(index) = self.queue.pop_front() {
            // In memorization mode, remove the card from the queue
            if !index.memorization_card {
                self.queue.push_back(index);
            } else {
                self.datasets[index.dataset].cards[index.card].metadata =
                    Some(VocabMetadata::default());
                self.has_changes = true;
            }
        }
    }

    pub fn next_card(&mut self, answer_correct: bool, deck_config: &DeckConfig) {
        let current_date = chrono::Local::now().naive_utc();

        let Some(current_item) = self.queue.pop_front() else {
            return;
        };

        let deck_durations = &deck_config.deck_intervals;

        let card_mut = &mut self.datasets[current_item.dataset].cards[current_item.card];
        let current_deck = card_mut.get_deck(current_item.reverse).unwrap_or(0);

        // If in memorization mode, just remove the card from the queue
        if current_item.memorization_card {
            card_mut.metadata = Some(VocabMetadata::default());
            self.has_changes = true;
            return;
        }

        if answer_correct {
            let new_deck = (current_deck + 1).min(deck_durations.len() as u8 - 1);
            card_mut.update_metadata(
                new_deck,
                current_date + deck_durations[new_deck as usize].0,
                current_item.reverse,
            );
        } else {
            let new_deck = (current_deck as i16 - 1).max(0) as u8;
            card_mut.update_metadata(
                new_deck,
                current_date + deck_durations[new_deck as usize].0,
                current_item.reverse,
            );
            self.queue.push_back(current_item);
        }
        self.has_changes = true;
    }

    #[inline]
    pub fn current_progress(&self) -> usize {
        self.total_tasks() - self.queue.len()
    }

    #[inline]
    pub fn total_tasks(&self) -> usize {
        self.total_due
    }

    pub fn save(&self) -> Result<(), std::io::Error> {
        for dataset in &self.datasets {
            let file_path = &dataset.file_path;
            let mut file = std::fs::File::create(file_path)?;
            writeln!(file, "{}\t{}", dataset.lang_a, dataset.lang_b)?;
            for card in &dataset.cards {
                let line = match card.metadata {
                    Some(ref metadata) => format!(
                        "{}\t{}\t{}\t{}\t{}\t{}",
                        card.word_a.base,
                        card.word_b.base,
                        metadata.deck,
                        metadata.due_date.format("%Y-%m-%d %H:%M:%S"),
                        metadata.deck_reverse,
                        metadata.due_date_reverse.format("%Y-%m-%d %H:%M:%S")
                    ),
                    None => format!("{}\t{}", card.word_a.base, card.word_b.base),
                };
                writeln!(file, "{}", line)?;
            }
        }
        Ok(())
    }

    pub fn from_files(
        file_paths: &[String],
        filter_mode: FilterMode,
        sorted: bool,
        limit: Option<usize>,
        memorization_config: &MemorizationConfig,
    ) -> Result<Self, VocaParseError> {
        let datasets = file_paths
            .iter()
            .map(|file_path| VocaCardDataset::from_file(file_path))
            .collect::<Result<Vec<_>, VocaParseError>>()?;
        Ok(VocaSession::new(
            datasets,
            filter_mode,
            sorted,
            limit,
            memorization_config,
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::model::voca_card::VocabWord;

    use super::*;

    #[test]
    fn test_sorting() {
        let card1 = Vocab {
            word_a: VocabWord::from_str("hello"),
            word_b: VocabWord::from_str("hola"),
            metadata: Some(VocabMetadata {
                deck: 1,
                due_date: chrono::NaiveDateTime::parse_from_str(
                    "2023-10-01 12:00:00",
                    "%Y-%m-%d %H:%M:%S",
                )
                .unwrap(),
                deck_reverse: 2,
                due_date_reverse: chrono::NaiveDateTime::parse_from_str(
                    "2024-10-01 13:00:00",
                    "%Y-%m-%d %H:%M:%S",
                )
                .unwrap(),
            }),
        };
        let card2 = Vocab {
            word_a: VocabWord::from_str("world"),
            word_b: VocabWord::from_str("mundo"),
            metadata: Some(VocabMetadata {
                deck: 2,
                due_date: chrono::NaiveDateTime::parse_from_str(
                    "2023-09-01 12:00:00",
                    "%Y-%m-%d %H:%M:%S",
                )
                .unwrap(),
                deck_reverse: 1,
                due_date_reverse: chrono::NaiveDateTime::parse_from_str(
                    "2024-09-01 13:00:00",
                    "%Y-%m-%d %H:%M:%S",
                )
                .unwrap(),
            }),
        };
        let card3 = Vocab {
            word_a: VocabWord::from_str("test"),
            word_b: VocabWord::from_str("prueba"),
            metadata: Some(VocabMetadata {
                deck: 1,
                due_date: chrono::NaiveDateTime::parse_from_str(
                    "2023-08-01 12:00:00",
                    "%Y-%m-%d %H:%M:%S",
                )
                .unwrap(),
                deck_reverse: 2,
                due_date_reverse: chrono::NaiveDateTime::parse_from_str(
                    "2024-08-01 13:00:00",
                    "%Y-%m-%d %H:%M:%S",
                )
                .unwrap(),
            }),
        };

        let dataset = VocaCardDataset {
            cards: vec![card1, card2, card3],
            file_path: "test.txt".to_string(),
            lang_a: "English".to_string(),
            lang_b: "Spanish".to_string(),
        };

        let session = VocaSession::new(
            vec![dataset],
            FilterMode::All,
            true,
            None,
            &MemorizationConfig::default(),
        );

        assert_eq!(session.queue.len(), 6);
        assert_eq!(session.queue[0].card, 2); // "test"
        assert_eq!(session.queue[1].card, 1); // "world"
        assert_eq!(session.queue[2].card, 0); // "hello"
    }

    #[test]
    fn vocab_validation() {
        let task = VocabTask {
            query: "hello",
            answer: "hola",
            answer_variants: &vec!["hola".to_string(), "saludo".to_string()],
            show_answer: false,
        };
        let val_config = ValidationConfig {
            error_tolerance: 1,
            tolerance_min_length: 3,
        };
        assert!(task.is_correct("hola", &val_config));
        assert!(task.is_correct("hola!", &val_config));
        assert!(task.is_correct("saludo", &val_config));
        assert!(!task.is_correct("hello", &val_config));
    }
}
