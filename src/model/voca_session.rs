use std::collections::VecDeque;

use chrono::Duration;

use crate::config::{DeckConfig, MemorizationConfig, ValidationConfig};

use super::voca_card::{VocaCardDataset, VocaParseError, VocabMetadata};
use std::io::Write;

pub struct VocabTask {
    pub query: String,
    pub answer: String,
    pub show_answer: bool,
}

impl VocabTask {
    pub fn is_correct(&self, answer: &str, val_config: &ValidationConfig) -> bool {
        if self.answer.len() < val_config.tolerance_min_length {
            return self.answer == answer;
        }
        edit_distance::edit_distance(&self.answer, answer) <= val_config.error_tolerance
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
        use_all: bool,
        limit: Option<usize>,
        memorization_config: &MemorizationConfig,
    ) -> Self {
        let mut queue_seen = VecDeque::new();
        let mut queue_reverse = VecDeque::new();
        let mut queue_unseen = VecDeque::new();
        // let mut queue_reverse = VecDeque::new();
        let current_date = chrono::Local::now().naive_utc();
        let mut num_cards = 0;
        for (i, dataset) in datasets.iter().enumerate() {
            for (j, card) in dataset.cards.iter().enumerate() {
                if let Some(limit) = limit {
                    if num_cards >= limit {
                        break;
                    }
                }

                if card.metadata.is_none() && memorization_config.do_memorization_round {
                    queue_unseen.push_back(VocabItem {
                        dataset: i,
                        card: j,
                        reverse: memorization_config.memorization_reversed,
                        memorization_card: true,
                    });
                }

                
                let add_to_queue = use_all
                    || !matches!(&card.metadata, Some(metadata) if metadata.due_date > current_date);
                if add_to_queue {
                    queue_seen.push_back(VocabItem {
                        dataset: i,
                        card: j,
                        reverse: false,
                        memorization_card: false,
                    });
                }
                let add_to_queue_reverse = use_all
                    || !matches!(&card.metadata, Some(metadata) if metadata.due_date_reverse > current_date);
                if add_to_queue_reverse {
                    queue_reverse.push_back(VocabItem {
                        dataset: i,
                        card: j,
                        reverse: true,
                        memorization_card: false,
                    });
                }
                if add_to_queue || add_to_queue_reverse {
                    num_cards += 1;
                }
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
                .map(|card| VocabTask {
                    query: if index.reverse {
                        card.word_b.clone()
                    } else {
                        card.word_a.clone()
                    },
                    answer: if index.reverse {
                        card.word_a.clone()
                    } else {
                        card.word_b.clone()
                    },
                    show_answer: index.memorization_card,
                })
        })
    }

    pub fn current_target_lang(&self) -> Option<String> {
        self.queue.front().and_then(|index| {
            self.datasets.get(index.dataset).map(|d| {
                if index.reverse {
                    d.lang_a.clone()
                } else {
                    d.lang_b.clone()
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
                self.datasets[index.dataset].cards[index.card].metadata = Some(VocabMetadata::default());
                self.has_changes = true;
            }
        }
    }

    pub fn next_card(&mut self, answer_correct: bool, deck_config: &DeckConfig) {
        let current_date = chrono::Local::now().naive_utc();

        let Some(current_item) = self.queue.pop_front() else {
            return;
        };

        let deck_durations = &deck_config.deck_durations;

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
                current_date + Duration::days(deck_durations[new_deck as usize] as i64),
                current_item.reverse,
            );
        } else {
            let new_deck = (current_deck as i16 - 1).max(0) as u8;
            card_mut.update_metadata(
                new_deck,
                current_date + Duration::days(deck_durations[new_deck as usize] as i64),
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
                        card.word_a,
                        card.word_b,
                        metadata.deck,
                        metadata.due_date.format("%Y-%m-%d %H:%M:%S"),
                        metadata.deck_reverse,
                        metadata.due_date_reverse.format("%Y-%m-%d %H:%M:%S")
                    ),
                    None => format!("{}\t{}", card.word_a, card.word_b),
                };
                writeln!(file, "{}", line)?;
            }
        }
        Ok(())
    }

    pub fn from_files(
        file_paths: &[String],
        use_all: bool,
        limit: Option<usize>,
        memorization_config: &MemorizationConfig,
    ) -> Result<Self, VocaParseError> {
        let datasets = file_paths
            .iter()
            .map(|file_path| VocaCardDataset::from_file(file_path))
            .collect::<Result<Vec<_>, VocaParseError>>()?;
        Ok(VocaSession::new(
            datasets,
            use_all,
            limit,
            memorization_config,
        ))
    }
}
