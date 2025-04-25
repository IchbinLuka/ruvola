use std::collections::VecDeque;

use chrono::Duration;

use crate::config::{DeckConfig, ValidationConfig};

use super::voca_card::{VocaCardDataset, VocaParseError};
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
    fn new(datasets: Vec<VocaCardDataset>, use_all: bool, limit: Option<usize>) -> Self {
        let mut queue_seen = VecDeque::new();
        let mut queue_reverse = VecDeque::new();
        let mut queue_unseen = VecDeque::new();
        // let mut queue_reverse = VecDeque::new();
        let current_date = chrono::Local::now().naive_utc();
        for (i, dataset) in datasets.iter().enumerate() {
            for (j, card) in dataset.cards.iter().enumerate() {
                if card.deck.is_none() {
                    queue_unseen.push_back(VocabItem {
                        dataset: i,
                        card: j,
                        reverse: false,
                        memorization_card: true,
                    });
                }

                if let Some(limit) = limit {
                    // TODO: In theory it could happen that the limit is exceeded by 1
                    if queue_seen.len() + queue_reverse.len() >= limit {
                        break;
                    }
                }
                let add_to_queue =
                    use_all || !matches!(card.due_date, Some(date) if date > current_date);
                if add_to_queue {
                    queue_seen.push_back(VocabItem {
                        dataset: i,
                        card: j,
                        reverse: false,
                        memorization_card: false,
                    });
                }
                let add_to_queue_reverse =
                    use_all || !matches!(card.due_date_reverse, Some(date) if date > current_date);
                if add_to_queue_reverse {
                    queue_reverse.push_back(VocabItem {
                        dataset: i,
                        card: j,
                        reverse: true,
                        memorization_card: false,
                    });
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
            }
        }
    }

    pub fn next_card(&mut self, answer_correct: bool, deck_config: &DeckConfig) {
        let current_date = chrono::Local::now().naive_utc();

        let Some(current_item) = self.queue.pop_front() else {
            return;
        };

        // If in memorization mode, just remove the card from the queue
        if current_item.memorization_card {
            return;
        }

        let deck_durations = &deck_config.deck_durations;

        let card_mut = &mut self.datasets[current_item.dataset].cards[current_item.card];
        let current_deck = card_mut.get_deck(current_item.reverse).unwrap_or(0);

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
        let current_date = chrono::Local::now().naive_utc();
        for dataset in &self.datasets {
            let file_path = &dataset.file_path;
            let mut file = std::fs::File::create(file_path)?;
            writeln!(file, "{}\t{}", dataset.lang_a, dataset.lang_b)?;
            for card in &dataset.cards {
                let line = format!(
                    "{}\t{}\t{}\t{}\t{}\t{}",
                    card.word_a,
                    card.word_b,
                    card.deck.unwrap_or(0),
                    card.due_date
                        .unwrap_or(current_date)
                        .format("%Y-%m-%d %H:%M:%S"),
                    card.deck_reverse.unwrap_or(0),
                    card.due_date_reverse
                        .unwrap_or(current_date)
                        .format("%Y-%m-%d %H:%M:%S")
                );
                writeln!(file, "{}", line)?;
            }
        }
        Ok(())
    }

    pub fn from_files(
        file_paths: &[String],
        use_all: bool,
        limit: Option<usize>,
    ) -> Result<Self, VocaParseError> {
        let datasets = file_paths
            .iter()
            .map(|file_path| VocaCardDataset::from_file(file_path))
            .collect::<Result<Vec<_>, VocaParseError>>()?;
        Ok(VocaSession::new(datasets, use_all, limit))
    }
}
