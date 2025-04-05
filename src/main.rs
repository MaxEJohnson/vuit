use std::fs;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::PathBuf;
use std::process::Command;

use dirs;

use ignore::WalkBuilder;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

use ratatui::{
    prelude::{Constraint, Direction, Layout},
    style::{Color, Style, Stylize},
    symbols::border,
    text::{Line, Text},
    widgets::{Block, List, ListState, Paragraph},
    DefaultTerminal, Frame,
};

use clap::Command as ClapCommand;

use serde::{Deserialize, Serialize};

#[derive(Debug, Default)]
pub struct Vuit {
    // Config
    config: VuitRC,
    colorscheme_index: usize,

    // Input
    typed_input: String,

    // Lists to Display
    file_list: Vec<String>,
    preview: Vec<String>,
    recent_files: Vec<String>,
    fd_list: Vec<String>,

    // State Variables
    switch_focus: bool,
    hltd_file: usize,
    file_list_state: ListState,
    recent_state: ListState,

    // Termination
    exit: bool,
}

impl Vuit {
    fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        // Initialize Focus to File List
        self.switch_focus = true;

        // Populate fd list
        self.run_fd_cmd();

        // Populate File list and set it's highlight index
        self.file_list = self.run_search_cmd();
        self.file_list_state.select(Some(self.hltd_file));

        if self.hltd_file >= self.file_list.len() && self.file_list.len() > 0 {
            self.hltd_file = self.file_list.len() - 1;
        }

        // Create Preview of Highlighted File
        self.preview = self.run_preview_cmd();

        // Start Vuit
        while !self.exit {
            terminal.draw(|frame| self.ui(frame))?;
            self.handle_events(terminal)?;
        }

        // Clear Terminal after close
        let _ = terminal.clear();

        Ok(())
    }

    fn ui(&mut self, frame: &mut Frame) {
        // Window Titles
        let search_title = Line::from(" Search ".underlined());
        let preview_title = Line::from(" Preview ".underlined());
        let file_list_title = Line::from(" Files ".underlined());
        let recent_files_title = Line::from(" Recent ".underlined());

        // Window Blocks
        let search_block = Block::bordered()
            .title(search_title.centered())
            .border_set(border::THICK);

        let preview_block = Block::bordered()
            .title(preview_title.centered())
            .border_set(border::THICK);

        let file_list_block = Block::bordered()
            .title(file_list_title.centered())
            .border_set(border::THICK);

        let recentfiles_block = Block::bordered()
            .title(recent_files_title.centered())
            .border_set(border::THICK);

        // Text/Paragraphs to Display
        let input = Text::from("> ".to_owned() + &self.typed_input);

        let search_para = Paragraph::new(input)
            .block(search_block)
            .style(Style::default().fg(grab_config_color(&self.config.colorscheme)));

        let file_list_list = List::new(self.file_list.to_owned())
            .block(file_list_block)
            .style(Style::default().fg(grab_config_color(&self.config.colorscheme)))
            .highlight_style(
                Style::default()
                    .fg(Color::White)
                    .bg(grab_config_color(&self.config.highlight_color)),
            );

        let preview_list = List::new(self.preview.to_owned())
            .block(preview_block)
            .style(Style::default().fg(grab_config_color(&self.config.colorscheme)));

        let recent_files_list = List::new(self.recent_files.to_owned())
            .block(recentfiles_block)
            .style(Style::default().fg(grab_config_color(&self.config.colorscheme)))
            .highlight_style(
                Style::default()
                    .fg(Color::White)
                    .bg(grab_config_color(&self.config.highlight_color)),
            );

        // Layout Description
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(95), Constraint::Percentage(5)])
            .split(frame.area());

        let top_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[0]);

        let left_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
            .split(top_chunks[0]);

        // Rendering of Windows
        frame.render_stateful_widget(recent_files_list, left_chunks[0], &mut self.recent_state);
        frame.render_stateful_widget(file_list_list, left_chunks[1], &mut self.file_list_state);
        frame.render_widget(preview_list, top_chunks[1]);
        frame.render_widget(search_para, chunks[1]);
    }

    fn handle_events(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        match event::read()? {
            Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                self.handle_key_event(key_event, terminal);
            }
            _ => {}
        };
        Ok(())
    }

    fn run_fd_cmd(&mut self) {
        let mut results = Vec::new();

        for result in WalkBuilder::new(".").standard_filters(true).build() {
            if let Ok(entry) = result {
                let path = entry.path();
                if path.is_file() {
                    if let Some(path_str) = path.to_str() {
                        results.push(path_str.to_string());
                    }
                }
            }
        }

        self.fd_list = results;
    }

    fn run_search_cmd(&mut self) -> Vec<String> {
        let matcher = SkimMatcherV2::default();

        let mut matches: Vec<(i64, &String)> = self
            .fd_list
            .iter()
            .filter_map(|item| {
                matcher
                    .fuzzy_match(item, &self.typed_input)
                    .map(|score| (score, item))
            })
            .collect();

        matches.sort_by(|a, b| b.0.cmp(&a.0));

        matches.into_iter().map(|(_, item)| item.clone()).collect()
    }

    fn clean_utf8_content(&mut self, content: &str) -> String {
        content
            .chars()
            .filter(|&c| c.is_ascii_graphic() || c == '\n' || c == ' ')
            .collect()
    }

    fn run_preview_cmd(&mut self) -> Vec<String> {
        let file_list = if self.switch_focus {
            &self.file_list
        } else {
            &self.recent_files
        };

        if file_list.is_empty() {
            return vec![];
        }

        let file_path = &file_list[self.hltd_file];

        match File::open(file_path) {
            Ok(file) => {
                let reader = BufReader::new(file);
                reader
                    .lines()
                    .take(50)
                    .filter_map(Result::ok)
                    .map(|line| self.clean_utf8_content(&line))
                    .collect::<Vec<String>>()
            }
            Err(_) => vec!["No Preview Available".to_string()],
        }
    }

    fn next_colorscheme(&mut self, terminal: &mut DefaultTerminal) {
        self.colorscheme_index = (self.colorscheme_index + 1) % COLORS.len();
        self.config.colorscheme = COLORS[self.colorscheme_index].to_string();
        self.config.highlight_color =
            COLORS[(self.colorscheme_index + 1) % COLORS.len()].to_string();

        let _ = terminal.draw(|frame| self.ui(frame));
    }

    fn handle_key_event(&mut self, key_event: KeyEvent, terminal: &mut DefaultTerminal) {
        // Many of the Match cases look redundant with the switch_focus, but for now it deals with
        // double borrow issues well enough, will refactor later

        match key_event {
            KeyEvent {
                code: KeyCode::Char(c),
                modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT,
                ..
            } => {
                // FZF search after each keyswipe

                self.typed_input.push(c);
                self.file_list = self.run_search_cmd();

                if self.file_list.len() == 0 {
                    return;
                }

                if self.switch_focus {
                    self.file_list_state.select(Some(self.hltd_file));
                    if self.hltd_file >= self.file_list.len() && self.file_list.len() > 0 {
                        self.hltd_file = self.file_list.len() - 1;
                    }
                    self.preview = self.run_preview_cmd();
                } else {
                    self.recent_state.select(Some(self.hltd_file));
                    if self.hltd_file >= self.recent_files.len() && self.recent_files.len() > 0 {
                        self.hltd_file = self.recent_files.len() - 1;
                    }
                }
            }
            KeyEvent {
                code: KeyCode::Backspace,
                ..
            } => {
                // FZF search after each backspace

                if self.typed_input.len() == 0 {
                    return;
                }

                self.typed_input.pop();
                self.file_list = self.run_search_cmd();

                if self.file_list.len() == 0 {
                    return;
                }

                if self.switch_focus {
                    self.file_list_state.select(Some(self.hltd_file));
                    if self.hltd_file >= self.file_list.len() && self.file_list.len() > 0 {
                        self.hltd_file = self.file_list.len() - 1;
                    }
                    self.preview = self.run_preview_cmd();
                } else {
                    self.recent_state.select(Some(self.hltd_file));
                    if self.hltd_file >= self.recent_files.len() && self.recent_files.len() > 0 {
                        self.hltd_file = self.recent_files.len() - 1;
                    }
                }
            }
            KeyEvent {
                code: KeyCode::Enter,
                ..
            } => {
                // Start editor on highlighted file when Enter is pressed
                if self.file_list.len() == 0 {
                    return;
                }

                if self.switch_focus {
                    if !self.recent_files.contains(&self.file_list[self.hltd_file]) {
                        self.recent_files
                            .push(self.file_list[self.hltd_file].to_owned());
                    }
                }

                if self.recent_files.len() > 5 {
                    self.recent_files.remove(0);
                }

                if self.switch_focus {
                    let _ = Command::new(&self.config.editor.to_string())
                        .arg(self.file_list[self.hltd_file].to_owned())
                        .status()
                        .expect("Failed to start selected editor");
                } else {
                    let _ = Command::new(&self.config.editor.to_string())
                        .arg(self.recent_files[self.hltd_file].to_owned())
                        .status()
                        .expect("Failed to start selected editor");
                }

                // Clear terminal on exit from editor
                let _ = terminal.clear();
                let _ = terminal.draw(|frame| self.ui(frame));
            }
            KeyEvent {
                code: KeyCode::Esc, ..
            } => {
                // Exit when Esc is pressed
                self.exit = true;
            }
            KeyEvent {
                code: KeyCode::Char('j'),
                modifiers: KeyModifiers::CONTROL,
                ..
            }
            | KeyEvent {
                code: KeyCode::Down,
                ..
            } => {
                // Navigate file list down
                if self.switch_focus {
                    if self.file_list.len() == 0 {
                        return;
                    }
                } else {
                    if self.recent_files.len() == 0 {
                        return;
                    }
                }

                self.hltd_file += 1;

                if self.switch_focus {
                    if self.hltd_file >= self.file_list.len() && self.file_list.len() > 0 {
                        self.hltd_file = self.file_list.len() - 1;
                    }
                    self.file_list_state.select(Some(self.hltd_file));
                    self.preview = self.run_preview_cmd();
                } else {
                    if self.hltd_file >= self.recent_files.len() && self.recent_files.len() > 0 {
                        self.hltd_file = self.recent_files.len() - 1;
                    }
                    self.recent_state.select(Some(self.hltd_file));
                    self.preview = self.run_preview_cmd();
                }
            }
            KeyEvent {
                code: KeyCode::Char('k') | KeyCode::Up,
                modifiers: KeyModifiers::CONTROL,
                ..
            }
            | KeyEvent {
                code: KeyCode::Up, ..
            } => {
                // Navigate file list up
                if self.switch_focus {
                    if self.file_list.len() == 0 {
                        return;
                    }
                } else {
                    if self.recent_files.len() == 0 {
                        return;
                    }
                }

                if self.hltd_file == 0 {
                    return;
                }

                self.hltd_file -= 1;

                if self.switch_focus {
                    if self.hltd_file >= self.file_list.len() && self.file_list.len() > 0 {
                        self.hltd_file = self.file_list.len() - 1;
                    }
                    self.file_list_state.select(Some(self.hltd_file));
                    self.preview = self.run_preview_cmd();
                } else {
                    if self.hltd_file >= self.recent_files.len() && self.recent_files.len() > 0 {
                        self.hltd_file = self.recent_files.len() - 1;
                    }
                    self.recent_state.select(Some(self.hltd_file));
                    self.preview = self.run_preview_cmd();
                }
            }
            KeyEvent {
                code: KeyCode::Tab, ..
            } => {
                // Switch between recent and search files
                if self.recent_files.len() == 0 {
                    return;
                }

                self.switch_focus = !self.switch_focus;

                if self.switch_focus {
                    self.recent_state.select(None);
                    self.hltd_file = 0;
                    self.file_list_state.select(Some(self.hltd_file));
                    if self.file_list.len() == 0 {
                        return;
                    }
                    self.preview = self.run_preview_cmd();
                } else {
                    self.file_list_state.select(None);
                    self.hltd_file = 0;
                    self.recent_state.select(Some(self.hltd_file));
                    if self.recent_files.len() == 0 {
                        return;
                    }
                    self.preview = self.run_preview_cmd();
                }
            }
            KeyEvent {
                code: KeyCode::Char('r'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                // Refresh list of available files (e.g. after adding a new file, etc, ...)
                self.run_fd_cmd();
            }
            KeyEvent {
                code: KeyCode::Char('n'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.next_colorscheme(terminal);
            }
            _ => {}
        };
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct VuitRC {
    colorscheme: String,
    highlight_color: String,
    editor: String,
}

// Eventually bake in default colorschemes like gruvbox, tokyonight, etc
fn grab_config_color(color_str: &str) -> Color {
    match color_str.to_lowercase().as_str() {
        "lightblue" => Color::LightBlue,
        "blue" => Color::Blue,
        "lightred" => Color::LightRed,
        "red" => Color::Green,
        "lightgreen" => Color::LightGreen,
        "green" => Color::Green,
        "lightcyan" => Color::LightCyan,
        "cyan" => Color::Cyan,
        "lightyellow" => Color::LightYellow,
        "yellow" => Color::Yellow,
        &_ => Color::LightBlue,
    }
}

impl Default for VuitRC {
    fn default() -> Self {
        Self {
            colorscheme: "lightblue".to_string(),
            highlight_color: "blue".to_string(),
            editor: "vim".to_string(),
        }
    }
}

fn expand_tilde(path: &str) -> PathBuf {
    if path.starts_with("~") {
        if let Some(home_dir) = dirs::home_dir() {
            return home_dir.join(&path[2..]); // Replace `~` with the home directory
        }
    }
    PathBuf::from(path)
}

// Needs to be workshopped, temporary for now
const COLORS: &[&str] = &[
    "lightblue",
    "cyan",
    "lightgreen",
    "yellow",
    "lightred",
    "green",
    "lightcyan",
    "blue",
    "lightyellow",
    "red",
];

fn main() -> io::Result<()> {
    // Versioning
    let matches = ClapCommand::new("vuit")
        .version(env!("CARGO_PKG_VERSION")) // Uses the version from Cargo.toml
        .about("Vim User Interface Terminal - A Buffer Manager for Vim")
        .get_matches();

    if matches.contains_id("version") {
        println!("vuit version {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    // Load Configuration of Vuit
    let vuitrc_path = expand_tilde("~/.vuit/.vuitrc");

    let contents = match fs::read_to_string(vuitrc_path) {
        Ok(contents) => contents,
        Err(_) => String::new(),
    };

    let config = if !contents.is_empty() {
        match serde_json::from_str::<VuitRC>(&contents) {
            Ok(config) => config,
            Err(e) => {
                eprintln!("Failed to parse JSON: {}", e);
                return Ok(());
            }
        }
    } else {
        VuitRC::default()
    };

    // Vuit App Start
    let mut terminal = ratatui::init();

    let vuit_app = &mut Vuit {
        config: config,
        ..Default::default()
    };

    let vuit_result = vuit_app.run(&mut terminal);
    ratatui::restore();
    vuit_result
}
