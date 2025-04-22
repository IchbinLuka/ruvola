use std::collections::VecDeque;

use chrono::{Duration, NaiveDateTime};
use cli_log;
use color_eyre::Result;
use edit_distance::edit_distance;
use ratatui::{
    DefaultTerminal, Frame,
    crossterm::event::{self, Event, KeyCode, KeyEventKind},
    layout::{Constraint, Layout, Position},
    style::{Color, Style, Stylize},
    symbols::Marker,
    text::{Line, Text},
    widgets::{
        Block, Paragraph,
        canvas::{Canvas, Rectangle},
    },
};
use std::io::{BufRead, Write}; // also import logging macros

const DECK_DURATIONS: [Duration; 5] = [
    Duration::days(1),
    Duration::days(3),
    Duration::days(7),
    Duration::days(14),
    Duration::days(30),
];

fn main() -> Result<()> {
    cli_log::init_cli_log!();
    color_eyre::install()?;
    let terminal = ratatui::init();
    let app_result = App::new().run(terminal);
    ratatui::restore();
    app_result
    // Ok(())
}

#[derive(Debug)]
struct VocaCardDataSet {
    cards: Vec<Vocab>,
    file_path: String,
}

#[derive(Debug)]
struct Vocab {
    word_a: String,
    word_b: String,
    due_date: Option<NaiveDateTime>,
    due_date_reverse: Option<NaiveDateTime>,
    deck: Option<u8>,
    deck_reverse: Option<u8>,
}

impl Vocab {
    fn update_metadata(&mut self, deck: u8, due_date: NaiveDateTime, reverse: bool) {
        if reverse {
            self.deck_reverse = Some(deck);
            self.due_date_reverse = Some(due_date);
        } else {
            self.deck = Some(deck);
            self.due_date = Some(due_date);
        }
    }

    fn get_deck(&self, reverse: bool) -> Option<u8> {
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
                let date_b = parts.next().and_then(|d| {
                    Some(
                        NaiveDateTime::parse_from_str(d, "%Y-%m-%d %H:%M:%S")
                            .expect("Failed to parse date"),
                    )
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

struct VocabTask {
    query: String,
    answer: String,
}

#[derive(Debug)]
struct VocabItem {
    dataset: usize,
    card: usize,
    reverse: bool,
}

struct VocaSession {
    datasets: Vec<VocaCardDataSet>,
    queue: VecDeque<VocabItem>,
}

impl VocaSession {
    fn new(datasets: Vec<VocaCardDataSet>) -> Self {
        let mut queue = VecDeque::new();
        let mut queue_reverse = VecDeque::new();
        // let mut queue_reverse = VecDeque::new();
        let current_date = chrono::Local::now().naive_utc();
        for (i, dataset) in datasets.iter().enumerate() {
            cli_log::info!("Dataset: {:?}", dataset);
            for (j, card) in dataset.cards.iter().enumerate() {
                let add_to_queue = !matches!(card.due_date, Some(date) if date > current_date);
                cli_log::info!("Card: {:?}, Due Date: {:?}", card, card.due_date);
                if add_to_queue {
                    queue.push_back(VocabItem {
                        dataset: i,
                        card: j,
                        reverse: false,
                    });
                }
                let add_to_queue_reverse =
                    !matches!(card.due_date_reverse, Some(date) if date > current_date);
                if add_to_queue_reverse {
                    queue_reverse.push_back(VocabItem {
                        dataset: i,
                        card: j,
                        reverse: true,
                    });
                }
            }
        }

        for item in queue_reverse {
            queue.push_back(item);
        }
        VocaSession { datasets, queue }
    }

    fn current_task(&self) -> Option<VocabTask> {
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
                })
        })
    }

    fn skip_card(&mut self) {
        if let Some(index) = self.queue.pop_front() {
            self.queue.push_back(index);
        }
    }

    fn is_correct(&self, answer: &str) -> bool {
        if let Some(current_task) = self.current_task() {
            edit_distance(&current_task.answer, answer) < 3
        } else {
            false
        }
    }

    fn next_card(&mut self, answer_correct: bool) {
        let current_date = chrono::Local::now().naive_utc();

        let Some(current_item) = self.queue.pop_front() else {
            return;
        };
        let card = &mut self.datasets[current_item.dataset].cards[current_item.card];
        let current_deck = card.get_deck(current_item.reverse).unwrap_or(0);

        if answer_correct {
            let new_deck = (current_deck + 1).min(DECK_DURATIONS.len() as u8 - 1);
            card.update_metadata(
                new_deck,
                current_date + DECK_DURATIONS[new_deck as usize],
                current_item.reverse,
            );
        } else {
            let new_deck = (current_deck as i16 - 1).max(0) as u8;
            card.update_metadata(
                new_deck,
                current_date + DECK_DURATIONS[new_deck as usize],
                current_item.reverse,
            );
            self.queue.push_back(current_item);
        }
    }

    #[inline]
    fn current_progress(&self) -> usize {
        self.total_tasks() - self.queue.len()
    }

    #[inline]
    fn total_tasks(&self) -> usize {
        self.datasets
            .iter()
            .map(|dataset| dataset.cards.len())
            .sum::<usize>()
            * 2usize
    }

    fn save(&self) -> Result<()> {
        let current_date = chrono::Local::now().naive_utc();
        for dataset in &self.datasets {
            let file_path = &dataset.file_path;
            let mut file = std::fs::File::create(file_path)?;
            for card in &dataset.cards {
                let line = format!(
                    "{}\t{}\t{}\t{}\t{}\t{}",
                    card.word_a,
                    card.word_b,
                    card.deck.unwrap_or(0),
                    card.due_date
                        .unwrap_or(current_date)
                        .format("%Y-%m-%d %H:%M:%S")
                        .to_string(),
                    card.deck_reverse.unwrap_or(0),
                    card.due_date_reverse
                        .unwrap_or(current_date)
                        .format("%Y-%m-%d %H:%M:%S")
                        .to_string()
                );
                writeln!(file, "{}", line)?;
            }
        }
        Ok(())
    }

    fn from_file(file_paths: &[&str]) -> Result<Self> {
        let mut datasets = Vec::new();
        for file_path in file_paths {
            let file = std::fs::File::open(file_path)?;
            let reader = std::io::BufReader::new(file);
            let mut cards = Vec::new();
            for line in reader.lines() {
                let line = line?;
                if !line.trim().is_empty() {
                    let card = Vocab::from_line(&line)?;
                    cards.push(card);
                }
            }
            datasets.push(VocaCardDataSet {
                cards,
                file_path: file_path.to_string(),
            });
        }
        Ok(VocaSession::new(datasets))
    }
}

// fn parse_file()

/// App holds the state of the application
struct App {
    input: String,
    cursor_pos: usize,
    input_mode: InputMode,
    voca_session: VocaSession,
    current_screen: CurrentScreen,
}

enum InputMode {
    Normal,
    Editing,
}

struct Review {
    correct: bool,
    answer: String,
    correct_answer: String,
}

enum CurrentScreen {
    Query,
    Review(Review),
}

impl App {
    fn new() -> App {
        App {
            input: String::new(),
            cursor_pos: 0,
            input_mode: InputMode::Normal,
            voca_session: VocaSession::new(vec![VocaCardDataSet {
                cards: vec![
                    Vocab::from_line("hello\tworld").unwrap(),
                    Vocab::from_line("foo\tbar\t2\t2023-10-02 12:00:00\t2\t2023-10-02 12:00:00")
                        .unwrap(),
                ],
                file_path: "vocab.txt".to_string(),
            }]),
            current_screen: CurrentScreen::Query,
        }
    }

    fn move_cursor_left(&mut self) {
        let cursor_moved_left = self.cursor_pos.saturating_sub(1);
        self.cursor_pos = self.clamp_cursor(cursor_moved_left);
    }

    fn move_cursor_right(&mut self) {
        let cursor_moved_right = self.cursor_pos.saturating_add(1);
        self.cursor_pos = self.clamp_cursor(cursor_moved_right);
    }

    fn enter_char(&mut self, new_char: char) {
        let index = self.byte_index();
        self.input.insert(index, new_char);
        self.move_cursor_right();
    }

    /// Returns the byte index based on the character position.
    ///
    /// Since each character in a string can be contain multiple bytes, it's necessary to calculate
    /// the byte index based on the index of the character.
    fn byte_index(&self) -> usize {
        self.input
            .char_indices()
            .map(|(i, _)| i)
            .nth(self.cursor_pos)
            .unwrap_or(self.input.len())
    }

    fn delete_char(&mut self) {
        let is_not_cursor_leftmost = self.cursor_pos != 0;
        if is_not_cursor_leftmost {
            // Method "remove" is not used on the saved text for deleting the selected char.
            // Reason: Using remove on String works on bytes instead of the chars.
            // Using remove would require special care because of char boundaries.

            let current_index = self.cursor_pos;
            let from_left_to_current_index = current_index - 1;

            // Getting all characters before the selected character.
            let before_char_to_delete = self.input.chars().take(from_left_to_current_index);
            // Getting all characters after selected character.
            let after_char_to_delete = self.input.chars().skip(current_index);

            // Put all characters together except the selected one.
            // By leaving the selected one out, it is forgotten and therefore deleted.
            self.input = before_char_to_delete.chain(after_char_to_delete).collect();
            self.move_cursor_left();
        }
    }

    fn clamp_cursor(&self, new_cursor_pos: usize) -> usize {
        new_cursor_pos.clamp(0, self.input.chars().count())
    }

    fn reset_cursor(&mut self) {
        self.cursor_pos = 0;
    }

    fn reset_input(&mut self) {
        self.input.clear();
        self.reset_cursor();
    }

    fn next_card(&mut self, correct: bool) {
        self.voca_session.next_card(correct);
        self.current_screen = CurrentScreen::Query;
        self.reset_input();
        self.input_mode = InputMode::Editing;
    }

    fn submit_message(&mut self) {
        let Some(correct_answer) = self
            .voca_session
            .current_task()
            .map(|card| card.answer.clone())
        else {
            return;
        };
        let answer = self.input.clone();
        let correct = self.voca_session.is_correct(answer.as_str());
        match &self.current_screen {
            CurrentScreen::Query => {
                self.current_screen = CurrentScreen::Review(Review {
                    correct,
                    answer: self.input.clone(),
                    correct_answer,
                });
            }
            CurrentScreen::Review(review) if correct => {
                self.next_card(review.correct);
            }
            _ => {}
        }
        if !correct {
            self.reset_input();
        }
        self.input_mode = InputMode::Normal;
    }

    fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        loop {
            let Some(current_card) = self.voca_session.current_task() else {
                self.voca_session.save()?;
                break Ok(());
            };
            terminal.draw(|frame| self.draw(frame, &current_card))?;

            if let Event::Key(key) = event::read()? {
                match self.input_mode {
                    InputMode::Normal => match key.code {
                        KeyCode::Char('e') => {
                            self.input_mode = InputMode::Editing;
                        }
                        KeyCode::Char('q') => {
                            return Ok(());
                        }
                        KeyCode::Enter => {
                            let CurrentScreen::Review(review) = &self.current_screen else {
                                continue;
                            };
                            if review.correct {
                                self.next_card(true);
                            }
                        }
                        KeyCode::Char('a') => {
                            let CurrentScreen::Review(review) = &self.current_screen else {
                                continue;
                            };
                            if !review.correct {
                                self.next_card(true);
                            }
                        }
                        KeyCode::Char('s') => {
                            self.reset_input();
                            self.voca_session.skip_card();
                        }
                        _ => {}
                    },
                    InputMode::Editing if key.kind == KeyEventKind::Press => match key.code {
                        KeyCode::Enter => self.submit_message(),
                        KeyCode::Char(to_insert) => self.enter_char(to_insert),
                        KeyCode::Backspace => self.delete_char(),
                        KeyCode::Left => self.move_cursor_left(),
                        KeyCode::Right => self.move_cursor_right(),
                        KeyCode::Esc => self.input_mode = InputMode::Normal,
                        _ => {}
                    },
                    InputMode::Editing => {}
                }
            }
        }
    }

    fn draw(&mut self, frame: &mut Frame, current_card: &VocabTask) {
        let vertical = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ]);
        let [help_area, prompt_area, progress] = vertical.margin(1).areas(frame.area());

        let horizontal = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Fill(1),
            Constraint::Fill(1),
        ]);

        let [vocab_prompt_area, input_area, correct_answer_area] = horizontal.areas(prompt_area);

        let is_review = matches!(self.current_screen, CurrentScreen::Review(Review { correct, .. }) if !correct);

        let msg = match self.input_mode {
            InputMode::Normal if is_review => {
                vec!["Press ".into(), "a".bold(), " to accept anyway".into()]
            }
            InputMode::Normal => vec![
                "Press ".into(),
                "q".bold(),
                " to exit, ".into(),
                "e".bold(),
                " to start editing, ".into(),
                "s".bold(),
                " to skip the card".into(),
            ],
            InputMode::Editing => vec![
                "Press ".into(),
                "Esc".bold(),
                " to stop editing, ".into(),
                "Enter".bold(),
                " to record the message".into(),
            ],
        };
        let text = Text::from(Line::from(msg));
        let help_message = Paragraph::new(text);
        frame.render_widget(help_message, help_area);

        let input = Paragraph::new(self.input.as_str())
            .style(match self.input_mode {
                InputMode::Normal => Style::default(),
                InputMode::Editing => Style::default().fg(Color::Yellow),
            })
            .block(Block::bordered().title("Input"));
        frame.render_widget(input, input_area);
        match self.input_mode {
            // Hide the cursor. `Frame` does this by default, so we don't need to do anything here
            InputMode::Normal => {}

            // Make the cursor visible and ask ratatui to put it at the specified coordinates after
            // rendering
            #[allow(clippy::cast_possible_truncation)]
            InputMode::Editing => frame.set_cursor_position(Position::new(
                input_area.x + self.cursor_pos as u16 + 1,
                input_area.y + 1,
            )),
        }

        frame.render_widget(
            Paragraph::new(current_card.query.to_string()).block(Block::bordered()),
            vocab_prompt_area,
        );
        frame.render_widget(
            format!(
                "{}/{}",
                self.voca_session.current_progress() + 1,
                self.voca_session.total_tasks()
            ),
            progress,
        );

        if let CurrentScreen::Review(Review {
            correct,
            correct_answer,
            ..
        }) = &self.current_screen
        {
            let area = frame.area();

            let canvas = Canvas::default()
                .x_bounds([0.0, area.width as f64])
                .y_bounds([0.0, area.height as f64])
                .marker(Marker::HalfBlock)
                .paint(|ctx| {
                    ctx.draw(&Rectangle {
                        x: 0.5,
                        y: 0.5,
                        width: (area.width - 1) as f64,
                        height: (area.height - 1) as f64,
                        color: if *correct { Color::Green } else { Color::Red },
                    });
                });
            frame.render_widget(canvas, area);
            frame.render_widget(
                Paragraph::new(correct_answer.to_string())
                    .block(Block::bordered().title("Correct Answer")),
                correct_answer_area,
            );
        }
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
