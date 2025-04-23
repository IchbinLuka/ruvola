use std::collections::VecDeque;

use chrono::{Duration, NaiveDateTime};
use clap::Parser;
use color_eyre::{Result, eyre::OptionExt};
use config::{AppConfig, DeckConfig, ValidationConfig};
use crossterm::execute;
use edit_distance::edit_distance;
use ratatui::{
    DefaultTerminal, Frame,
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    layout::{Constraint, Flex, Layout, Position},
    style::{Color, Style, Stylize},
    symbols::Marker,
    text::{Line, Span, Text},
    widgets::{
        Block, Clear, List, Padding, Paragraph, Row, Table, Widget,
        canvas::{Canvas, Rectangle},
    },
};
use std::io::{BufRead, Write};

mod config;

fn main() -> Result<()> {
    let args = Arguments::parse();
    cli_log::init_cli_log!();
    color_eyre::install()?;

    let mut terminal = ratatui::init();
    // Set cursor style to steady bar
    execute!(
        terminal.backend_mut(),
        crossterm::cursor::SetCursorStyle::SteadyBar
    )?;

    let config = config::AppConfig::load_from_file()?;
    let session = VocaSession::from_files(&args.file_paths, args.all, args.limit)?;
    let app_result = App::new(config, session).run(terminal);
    ratatui::restore();
    app_result
}

#[derive(clap::Parser, Debug)]
#[clap(name = "vocab_trainer", version, about)]
struct Arguments {
    /// Limit for the number of cards to show
    #[arg(short, long)]
    limit: Option<usize>,
    /// Show all cards, even if they are not due
    #[arg(short, long)]
    all: bool,
    /// Paths to the vocab files
    file_paths: Vec<String>,
}

#[derive(Debug)]
struct VocaCardDataset {
    cards: Vec<Vocab>,
    file_path: String,
    lang_a: String,
    lang_b: String,
}

impl VocaCardDataset {
    fn from_file(file_path: &str) -> Result<Self> {
        let file = std::fs::File::open(file_path)?;
        let reader = std::io::BufReader::new(file);
        let mut cards = Vec::new();
        let mut lines = reader.lines();
        let header = lines.next().ok_or_eyre("Empty file")??;
        let mut parts = header.split('\t');
        let lang_a = parts
            .next()
            .ok_or_eyre("Missing language header")?
            .to_string();
        let lang_b = parts.next().ok_or_eyre("Invalid header")?.to_string();
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

struct VocabTask {
    query: String,
    answer: String,
}

impl VocabTask {
    fn is_correct(&self, answer: &str, val_config: &ValidationConfig) -> bool {
        if self.answer.len() < val_config.tolerance_min_length {
            return self.answer == answer;
        }
        edit_distance(&self.answer, answer) <= val_config.error_tolerance
    }
}

#[derive(Debug)]
struct VocabItem {
    dataset: usize,
    card: usize,
    reverse: bool,
}

struct VocaSession {
    datasets: Vec<VocaCardDataset>,
    queue: VecDeque<VocabItem>,
    has_changes: bool,
    total_due: usize,
}

impl VocaSession {
    fn new(datasets: Vec<VocaCardDataset>, use_all: bool, limit: Option<usize>) -> Self {
        let mut queue = VecDeque::new();
        let mut queue_reverse = VecDeque::new();
        // let mut queue_reverse = VecDeque::new();
        let current_date = chrono::Local::now().naive_utc();
        for (i, dataset) in datasets.iter().enumerate() {
            for (j, card) in dataset.cards.iter().enumerate() {
                if let Some(limit) = limit {
                    // TODO: In theory it could happen that the limit is exceeded by 1
                    if queue.len() + queue_reverse.len() >= limit {
                        break;
                    }
                }
                let add_to_queue =
                    use_all || !matches!(card.due_date, Some(date) if date > current_date);
                if add_to_queue {
                    queue.push_back(VocabItem {
                        dataset: i,
                        card: j,
                        reverse: false,
                    });
                }
                let add_to_queue_reverse =
                    use_all || !matches!(card.due_date_reverse, Some(date) if date > current_date);
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
        let total_due = queue.len();
        VocaSession {
            datasets,
            queue,
            has_changes: false,
            total_due,
        }
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

    fn current_target_lang(&self) -> Option<String> {
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

    fn skip_card(&mut self) {
        if let Some(index) = self.queue.pop_front() {
            self.queue.push_back(index);
        }
    }

    fn next_card(&mut self, answer_correct: bool, deck_config: &DeckConfig) {
        let current_date = chrono::Local::now().naive_utc();

        let Some(current_item) = self.queue.pop_front() else {
            return;
        };

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
    fn current_progress(&self) -> usize {
        self.total_tasks() - self.queue.len()
    }

    #[inline]
    fn total_tasks(&self) -> usize {
        self.total_due
    }

    fn save(&self) -> Result<()> {
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

    fn from_files(file_paths: &[String], use_all: bool, limit: Option<usize>) -> Result<Self> {
        let datasets = file_paths
            .iter()
            .map(|file_path| VocaCardDataset::from_file(file_path))
            .collect::<Result<Vec<_>>>()?;
        Ok(VocaSession::new(datasets, use_all, limit))
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
    special_letters_popup: Option<SpecialLettersPopup>,
    config: config::AppConfig,
    show_help: bool,
}

enum InputMode {
    Normal,
    Editing,
}

struct Review {
    correct: bool,
    correct_answer: String,
}

enum CurrentScreen {
    Query,
    Review(Review),
}

enum KeyHandleResult {
    Quit { save: bool },
    None,
}

impl App {
    fn new(config: AppConfig, session: VocaSession) -> App {
        App {
            input: String::new(),
            cursor_pos: 0,
            input_mode: InputMode::Normal,
            voca_session: session,
            current_screen: CurrentScreen::Query,
            special_letters_popup: None,
            show_help: false,
            config,
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

    fn on_char_input(&mut self, c: char, modifiers: KeyModifiers) {
        let Some(target_lang) = self.voca_session.current_target_lang() else {
            return;
        };
        if modifiers.contains(KeyModifiers::CONTROL) {
            let Some(lang_chars) = self.config.special_letters.0.get(&target_lang) else {
                return;
            };
            let popup = match c {
                ' ' => {
                    let letters = lang_chars
                        .iter()
                        .flat_map(|s| s.special.iter())
                        .cloned()
                        .collect();
                    Some(SpecialLettersPopup { letters })
                }
                c => lang_chars
                    .iter()
                    .find(|s| s.base == c.to_string())
                    .map(|s| SpecialLettersPopup {
                        letters: s.special.to_vec(),
                    }),
            };
            self.special_letters_popup = popup;
        } else {
            let index = self.byte_index();
            self.input.insert(index, c);
            self.move_cursor_right();
        }
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
        self.voca_session
            .next_card(correct, &self.config.deck_config);
        self.current_screen = CurrentScreen::Query;
        self.reset_input();
        self.input_mode = if self.voca_session.current_task().is_some() {
            InputMode::Editing
        } else {
            InputMode::Normal
        };
    }

    fn submit_message(&mut self) {
        let Some(current_task) = self.voca_session.current_task() else {
            return;
        };
        let correct_answer = current_task.answer.clone();
        let answer = self.input.clone();
        let correct = current_task.is_correct(answer.as_str(), &self.config.validation);
        match &self.current_screen {
            CurrentScreen::Query => {
                self.current_screen = CurrentScreen::Review(Review {
                    correct,
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

    fn handle_key_events(&mut self, event: KeyEvent) -> KeyHandleResult {
        match self.input_mode {
            InputMode::Normal => match event.code {
                KeyCode::Char('e') => {
                    self.input_mode = InputMode::Editing;
                }
                KeyCode::Char('Q') => {
                    return KeyHandleResult::Quit { save: false };
                }
                KeyCode::Char('w') => {
                    return KeyHandleResult::Quit { save: true };
                }
                KeyCode::Enter => {
                    if let CurrentScreen::Review(review) = &self.current_screen {
                        if review.correct {
                            self.next_card(true);
                        }
                    }
                }
                KeyCode::Char('a') => {
                    if let CurrentScreen::Review(review) = &self.current_screen {
                        if !review.correct {
                            self.next_card(true);
                        }
                    }
                }
                KeyCode::Char('s') => {
                    self.reset_input();
                    self.voca_session.skip_card();
                }
                KeyCode::Char('h') => {
                    self.show_help = !self.show_help;
                }
                _ => {}
            },
            InputMode::Editing if event.kind == KeyEventKind::Press => match event.code {
                KeyCode::Enter => self.submit_message(),
                KeyCode::Char(c) => self.on_char_input(c, event.modifiers),
                KeyCode::Backspace => self.delete_char(),
                KeyCode::Left => self.move_cursor_left(),
                KeyCode::Right => self.move_cursor_right(),
                KeyCode::Esc => self.input_mode = InputMode::Normal,
                _ => {}
            },
            InputMode::Editing => {}
        };
        KeyHandleResult::None
    }

    fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        loop {
            terminal.draw(|frame| self.draw(frame))?;

            if let Some(popup) = &mut self.special_letters_popup {
                let result = popup.handle_events();
                match result {
                    Ok(SpecialLettersEventResult::Insert(s)) => {
                        self.input.insert_str(self.byte_index(), &s);
                        self.special_letters_popup = None;
                        self.cursor_pos = self.clamp_cursor(self.cursor_pos + s.len());
                    }
                    Ok(SpecialLettersEventResult::Cancel) => {
                        self.special_letters_popup = None;
                    }
                    Ok(SpecialLettersEventResult::Ignore) => {}
                    Err(_) => {}
                }
                continue;
            }

            if let Event::Key(key) = event::read()? {
                match self.handle_key_events(key) {
                    KeyHandleResult::Quit { save } => {
                        if save {
                            self.voca_session.save()?;
                        }
                        break Ok(());
                    }
                    KeyHandleResult::None => {}
                }
            }
        }
    }

    fn draw(&mut self, frame: &mut Frame) {
        let Some(current_card) = self.voca_session.current_task() else {
            frame.render_widget(
                NoCardsLeftScreen {
                    has_changes: self.voca_session.has_changes,
                },
                frame.area(),
            );
            return;
        };

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
            InputMode::Normal => vec!["Press ".into(), "h".bold(), " to show keybinds".into()],
            InputMode::Editing => vec![
                "Press ".into(),
                "Esc".bold(),
                " to stop editing, ".into(),
                "Enter".bold(),
                " to submit".into(),
            ],
        };
        let text = Text::from(Line::from(msg));
        let help_message = Paragraph::new(text);
        frame.render_widget(help_message, help_area);

        let input = Paragraph::new(self.input.as_str())
            .style(match self.input_mode {
                InputMode::Normal => Style::default(),
                InputMode::Editing => Style::default().fg(Color::LightBlue),
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
                        x: 0.0,
                        y: 0.0,
                        width: area.width as f64,
                        height: area.height as f64,
                        color: if *correct { Color::Green } else { Color::Red },
                    });
                });
            frame.render_widget(canvas, area);
            frame.render_widget(
                Paragraph::new(correct_answer.to_string())
                    .block(Block::bordered().title("Correct Answer")),
                correct_answer_area,
            );
        } else {
            frame.render_widget(Block::bordered(), correct_answer_area);
        }

        if let Some(popup) = &self.special_letters_popup {
            popup.draw(frame);
        }
        if self.show_help {
            HelpWidget.draw(frame);
        }
    }
}

struct SpecialLettersPopup {
    letters: Vec<String>,
}

enum SpecialLettersEventResult {
    Insert(String),
    Cancel,
    Ignore,
}

impl SpecialLettersPopup {
    fn handle_events(&self) -> Result<SpecialLettersEventResult> {
        const IGNORE: Result<SpecialLettersEventResult> = Ok(SpecialLettersEventResult::Ignore);
        let Event::Key(key) = event::read()? else {
            return IGNORE;
        };
        if let KeyCode::Esc = key.code {
            return Ok(SpecialLettersEventResult::Cancel);
        }
        let KeyCode::Char(ch) = key.code else {
            return IGNORE;
        };
        let radix = self.letters.len() as u32 + 1;
        if !ch.is_digit(radix) {
            return IGNORE;
        }
        let digit = ch.to_digit(radix).expect("Invalid digit") as i32 - 1;
        if digit >= self.letters.len() as i32 || digit < 0 {
            return IGNORE;
        }
        Ok(SpecialLettersEventResult::Insert(
            self.letters[digit as usize].clone(),
        ))
    }

    fn draw(&self, frame: &mut Frame) {
        let [area] = Layout::horizontal([Constraint::Percentage(30)])
            .flex(Flex::Center)
            .areas(frame.area());
        let [_, area] = Layout::vertical([Constraint::Percentage(70), Constraint::Percentage(30)])
            .flex(Flex::Center)
            .areas(area);

        frame.render_widget(Clear, area);
        frame.render_widget(Block::bordered().title("Special Letters"), area);

        const MAX_NUM_COLUMNS: usize = 3;
        let num_columns = self.letters.len().min(MAX_NUM_COLUMNS);
        let subareas = Layout::horizontal(
            (0..num_columns)
                .map(|_| Constraint::Fill(1))
                .collect::<Vec<_>>(),
        )
        .margin(1)
        .split(area);

        for (i, subarea) in subareas.iter().enumerate() {
            let items = self
                .letters
                .iter()
                .enumerate()
                .skip(i)
                .step_by(num_columns)
                .map(|(i, s)| format!("{:x}. {}", i + 1, s));
            let list = List::new(items);
            frame.render_widget(list, *subarea);
        }
    }
}

struct NoCardsLeftScreen {
    has_changes: bool,
}

impl Widget for NoCardsLeftScreen {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer)
    where
        Self: Sized,
    {
        let title = Text::raw("No cards left!").bold();

        let [title_area, _, keys_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(2),
        ])
        .flex(Flex::Center)
        .areas(area);

        let [title_area] = Layout::horizontal([Constraint::Length(title.width() as u16)])
            .flex(Flex::Center)
            .areas(title_area);
        title.render(title_area, buf);

        let keys = Text::raw(if self.has_changes {
            "Press 'w' to save changes and exit\nPress 'Q' to exit without saving"
        } else {
            "Press 'Q' to exit"
        });

        let [keys_area] = Layout::horizontal([Constraint::Length(keys.width() as u16)])
            .flex(Flex::Center)
            .areas(keys_area);
        keys.render(keys_area, buf);
    }
}

struct HelpWidget;

impl HelpWidget {
    fn draw(&self, frame: &mut Frame) {
        const KEYBINDINGS: [(&str, &str); 7] = [
            ("Q", "Quit without saving"),
            ("w", "Save and quit"),
            ("a", "Accept anyway"),
            ("Esc", "Stop editing"),
            ("Ctrl+Space", "Show special letters (in edit mode)"),
            ("e", "Enter edit mode"),
            ("s", "Skip"),
        ];
        let rows = KEYBINDINGS
            .iter()
            .map(|(key, desc)| {
                let key = Text::from(Line::from(vec![key.bold(), ": ".into()]));
                let desc = Text::from(Into::<Span<'_>>::into(*desc));
                Row::new([key, desc])
            })
            .collect::<Vec<_>>();

        let keys_width = KEYBINDINGS
            .iter()
            .map(|(key, _)| key.len())
            .max()
            .unwrap_or(0) as u16
            + 1;
        let desc_width = KEYBINDINGS.iter().map(|(_, d)| d.len()).max().unwrap_or(0) as u16;
        let table = Table::new(
            rows,
            [
                Constraint::Length(keys_width),
                Constraint::Length(desc_width),
            ],
        )
        .block(
            Block::bordered()
                .title("Keybindings")
                .padding(Padding::uniform(1)),
        );

        let [help_area] = Layout::horizontal([Constraint::Max(keys_width + desc_width + 3)])
            .flex(Flex::Center)
            .areas(frame.area());
        let [help_area] = Layout::vertical([Constraint::Max(KEYBINDINGS.len() as u16 + 2)])
            .flex(Flex::Center)
            .areas(help_area);
        frame.render_widget(Clear, help_area);
        frame.render_widget(table, help_area);
    }
}

impl Widget for HelpWidget {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer)
    where
        Self: Sized,
    {
        const KEYBINDINGS: [(&str, &str); 7] = [
            ("Q", "Quit without saving"),
            ("w", "Save and quit"),
            ("a", "Accept anyway"),
            ("Esc", "Stop editing"),
            ("Ctrl+Space", "Show special letters (in edit mode)"),
            ("e", "Enter edit mode"),
            ("s", "Skip"),
        ];

        Clear.render(area, buf);
        List::new(KEYBINDINGS.iter().map(|(key, desc)| {
            Text::from(Line::from(vec![key.bold(), ": ".into(), (*desc).into()]))
        }))
        .block(Block::bordered().title("Keybindings"))
        .render(area, buf);
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
