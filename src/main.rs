use anyhow::Result;
use clap::Parser;
use config::AppConfig;
use crossterm::execute;
use model::voca_session::VocaSession;
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

mod config;
mod model;

fn main() -> Result<()> {
    let args = Arguments::parse();
    cli_log::init_cli_log!();
    let config = config::AppConfig::load_from_config_file(args.override_config_file.as_deref())?;
    let session = VocaSession::from_files(
        &args.file_paths,
        (&args).try_into()?,
        args.sort,
        args.limit,
        &config.memorization,
    )?;
    let mut terminal = ratatui::init();
    // Set cursor style to steady bar
    execute!(
        terminal.backend_mut(),
        crossterm::cursor::SetCursorStyle::SteadyBar
    )?;

    let app_result = App::new(config, session).run(terminal);
    ratatui::restore();
    app_result
}

#[derive(clap::Parser, Debug)]
#[clap(name = "ruvola", version, about)]
struct Arguments {
    /// Limit for the number of distinct cards to show. Note that the actual number of tasks presented
    /// may be higher since both directions are tested and a potential memorization round.
    #[arg(short, long)]
    limit: Option<usize>,
    /// Show all cards, even if they are not due
    #[arg(short, long)]
    ignore_date: bool,
    /// Show only cards that have been seen before
    #[arg(long)]
    only_seen: bool,
    /// Show only new cards
    #[arg(long)]
    only_unseen: bool,
    /// Sort the cards by their due date
    #[arg(short, long)]
    sort: bool,
    /// Path to a local config file that overrides attributes of the global config file
    #[arg(long)]
    override_config_file: Option<String>,
    /// Paths to the vocab files
    file_paths: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum FilterMode {
    Normal,
    All,
    Seen,
    Unseen,
}

impl TryFrom<&Arguments> for FilterMode {
    type Error = anyhow::Error;

    fn try_from(args: &Arguments) -> Result<Self> {
        if [args.only_seen, args.only_unseen, args.ignore_date]
            .iter()
            .filter(|&&x| x)
            .count()
            > 1
        {
            return Err(anyhow::anyhow!(
                "Only one of --only-seen, --only-unseen, or --ignore-date can be specified"
            ));
        }
        Ok(if args.only_seen {
            FilterMode::Seen
        } else if args.only_unseen {
            FilterMode::Unseen
        } else if args.ignore_date {
            FilterMode::All
        } else {
            FilterMode::Normal
        })
    }
}

/// App holds the state of the application
struct App {
    input: String,
    cursor_pos: usize,
    input_mode: InputMode,
    voca_session: VocaSession,
    current_screen: CurrentScreen,
    popup: Option<Box<dyn Popup>>,
    config: config::AppConfig,
}

enum InputMode {
    Normal,
    Editing,
}

enum CurrentScreen {
    Query,
    Review { correct: bool },
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
            popup: None,
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
            self.popup = popup.map(|p| Box::new(p) as Box<dyn Popup>);
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
        if self.cursor_pos == 0 {
            return;
        }
        // "remove" method works with byte positions, so delete manually
        let current_index = self.cursor_pos;
        let from_left_to_current_index = current_index - 1;

        let before_char_to_delete = self.input.chars().take(from_left_to_current_index);
        let after_char_to_delete = self.input.chars().skip(current_index);

        // Put the string back together without the character to delete
        self.input = before_char_to_delete.chain(after_char_to_delete).collect();
        self.move_cursor_left();
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
        let answer = self.input.clone();
        let correct = current_task.is_correct(answer.as_str(), &self.config.validation);
        match &self.current_screen {
            CurrentScreen::Query => {
                self.current_screen = CurrentScreen::Review { correct };
            }
            CurrentScreen::Review { correct: r_correct } if correct => {
                self.next_card(*r_correct);
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
                    if let CurrentScreen::Review { correct: true } = &self.current_screen {
                        return KeyHandleResult::None;
                    }
                    self.input_mode = InputMode::Editing;
                }
                KeyCode::Char('Q') => {
                    return KeyHandleResult::Quit { save: false };
                }
                KeyCode::Char('w') => {
                    return KeyHandleResult::Quit { save: true };
                }
                KeyCode::Enter => {
                    if let CurrentScreen::Review { correct: true } = &self.current_screen {
                        self.next_card(true);
                    }
                }
                KeyCode::Char('a') => {
                    if let CurrentScreen::Review { correct } = &self.current_screen {
                        if !correct {
                            self.next_card(true);
                        }
                    }
                }
                KeyCode::Char('r') => {
                    if let CurrentScreen::Review { correct } = &self.current_screen {
                        if *correct {
                            self.next_card(false);
                        }
                    }
                }
                KeyCode::Char('s') if matches!(self.current_screen, CurrentScreen::Query) => {
                    self.reset_input();
                    self.voca_session.skip_card();
                }
                KeyCode::Char('h') => {
                    self.popup = Some(Box::new(HelpWidget));
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
            let event = event::read()?;
            if let Some(popup) = &mut self.popup {
                let result = popup.handle_events(event);
                match result {
                    PopupEventResult::Insert(s) => {
                        self.input.insert_str(self.byte_index(), &s);
                        self.popup = None;
                        self.cursor_pos = self.clamp_cursor(self.cursor_pos + s.len());
                    }
                    PopupEventResult::Cancel => {
                        self.popup = None;
                    }
                    PopupEventResult::Ignore => {}
                }
                continue;
            }

            if let Event::Key(key) = event {
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
                    has_changes: self.voca_session.has_changes(),
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

        let msg = match self.input_mode {
            InputMode::Normal => match self.current_screen {
                CurrentScreen::Review { correct } => {
                    if correct {
                        vec!["Press ".into(), "r".bold(), " to reject anyway".into()]
                    } else {
                        vec!["Press ".into(), "a".bold(), " to accept anyway".into()]
                    }
                }
                _ => vec!["Press ".into(), "h".bold(), " to show keybinds".into()],
            },
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
            InputMode::Normal => {}
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

        if let CurrentScreen::Review { correct } = &self.current_screen {
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
        }

        if matches!(self.current_screen, CurrentScreen::Review { .. }) || current_card.show_answer {
            frame.render_widget(
                Paragraph::new(current_card.answer.to_string())
                    .block(Block::bordered().title("Correct Answer")),
                correct_answer_area,
            );
        } else {
            frame.render_widget(Block::bordered(), correct_answer_area);
        }

        if let Some(popup) = &self.popup {
            popup.draw(frame);
        }
    }
}

trait Popup {
    fn handle_events(&self, event: Event) -> PopupEventResult;
    fn draw(&self, frame: &mut Frame);
}

struct SpecialLettersPopup {
    letters: Vec<String>,
}

enum PopupEventResult {
    Insert(String),
    Cancel,
    Ignore,
}

impl Popup for SpecialLettersPopup {
    fn handle_events(&self, event: Event) -> PopupEventResult {
        const IGNORE: PopupEventResult = PopupEventResult::Ignore;
        let Event::Key(key) = event else {
            return IGNORE;
        };
        if let KeyCode::Esc = key.code {
            return PopupEventResult::Cancel;
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
        PopupEventResult::Insert(self.letters[digit as usize].clone())
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

impl Popup for HelpWidget {
    fn handle_events(&self, event: Event) -> PopupEventResult {
        let Event::Key(key) = event else {
            return PopupEventResult::Ignore;
        };
        match key.code {
            KeyCode::Esc | KeyCode::Char('h') => PopupEventResult::Cancel,
            _ => PopupEventResult::Ignore,
        }
    }

    fn draw(&self, frame: &mut Frame) {
        const KEYBINDINGS: [(&str, &str); 9] = [
            ("Q", "Quit without saving"),
            ("w", "Save and quit"),
            ("a", "Accept anyway"),
            ("r", "Reject anyway"),
            ("Esc", "Stop editing"),
            ("Ctrl+Space", "Show all special letters (in edit mode)"),
            (
                "Ctrl+<Key>",
                "Show special letters for <Key> (in edit mode)",
            ),
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

        let [help_area] = Layout::horizontal([Constraint::Max(keys_width + desc_width + 5)])
            .flex(Flex::Center)
            .areas(frame.area());
        let [help_area] = Layout::vertical([Constraint::Max(KEYBINDINGS.len() as u16 + 4)])
            .flex(Flex::Center)
            .areas(help_area);
        frame.render_widget(Clear, help_area);
        frame.render_widget(table, help_area);
    }
}
