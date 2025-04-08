use std::fs;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use ignore::WalkBuilder;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use portable_pty::{unix::UnixPtySystem, CommandBuilder, PtySize, PtySystem};

use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

use ratatui::{
    prelude::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    symbols::border,
    text::{Line, Text},
    widgets::{Block, List, ListState, Paragraph},
    DefaultTerminal, Frame,
};

use clap::Command as ClapCommand;

use regex::Regex;

use serde::{Deserialize, Serialize};

// Constants for number of lines for each section
const RECENT_BUFFERS_NUM_LINES: u16 = 8;
const TERMINAL_NUM_LINES: u16 = 20;
const SEARCH_BAR_NUM_LINES: u16 = 3;
const PREVIEW_NUM_LINES: u16 = 50;
const HELP_TEXT_BOX_NUM_LINES: u16 = 18;

fn clean_utf8_content(content: &str) -> String {
    content
        .chars()
        .filter(|&c| c.is_ascii_graphic() || c == '\n' || c == ' ')
        .collect()
}

fn remove_ansi_escape_codes(input: &str) -> String {
    // Create a regex to match ANSI escape sequences
    let re = Regex::new(r"\x1b\[([0-9]{1,2};[0-9]{1,2}|[0-9]{1,2})?m").unwrap();
    let reclean = re.replace_all(input, "");
    let reclean = reclean.replace("\r", ""); // Remove carriage returns
    let reclean = reclean.replace("\t", "    "); // Convert tabs to spaces

    // Return the cleaned output
    reclean.to_string()
}

#[derive(Default)]
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
    term_out: String,
    help_menu: Vec<String>,

    // Terminal vars
    bash_process: Option<Box<dyn portable_pty::Child + Send + Sync>>,
    process_out: Arc<Mutex<Vec<String>>>,
    command_sender: Arc<Mutex<Option<Box<dyn Write + Send>>>>,

    // State Variables
    switch_focus: bool,
    toggle_terminal: bool,
    toggle_help_menu: bool,
    hltd_file: usize,
    file_list_state: ListState,
    recent_state: ListState,
    help_menu_state: ListState,

    // Termination
    exit: bool,
}

impl Vuit {
    fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        // Initialize Focus to File List
        self.switch_focus = true;
        self.toggle_terminal = false;
        self.toggle_help_menu = false;

        // Populate fd list
        self.run_fd_cmd();

        // Populate File list and set it's highlight index
        self.file_list = self.run_search_cmd();
        self.file_list_state.select(Some(self.hltd_file));

        if self.hltd_file >= self.file_list.len() && !self.file_list.is_empty() {
            self.hltd_file = self.file_list.len() - 1;
        }

        // Create Preview of Highlighted File
        self.preview = self.run_preview_cmd();

        // Start terminal Process
        self.start_term();

        // Start Vuit
        while !self.exit {
            terminal.draw(|frame| self.ui(frame))?;
            self.handle_events(terminal)?;
        }

        // Clear Terminal after close
        let _ = terminal.clear();

        Ok(())
    }

    fn start_term(&mut self) {
        let pty_system = UnixPtySystem::default();
        let pair = pty_system
            .openpty(PtySize {
                rows: 20,
                cols: 200,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("Failed to open PTY");

        let cmd = CommandBuilder::new("bash");

        // Set environment variables

        let child = pair.slave.spawn_command(cmd).expect("Failed to spawn bash");

        let reader = BufReader::new(pair.master.try_clone_reader().unwrap());
        let writer = pair.master.take_writer().unwrap();

        let output = self.process_out.clone();

        thread::spawn(move || {
            for line in reader.lines() {
                if let Ok(line_str) = line {
                    let mut output = output.lock().unwrap();
                    output.push(line_str);
                }
            }
        });

        self.bash_process = Some(child);
        self.command_sender = Arc::new(Mutex::new(Some(Box::new(writer))));
    }

    fn restart_terminal_session(&mut self) {
        // Kill the existing bash process if it exists
        if let Some(mut child) = self.bash_process.take() {
            child.kill().expect("Failed to kill bash process");
        }

        // Wait a bit before restarting to ensure it's clean
        thread::sleep(Duration::from_secs(1));

        // Start a new bash session
        self.start_term();
    }

    fn send_cmd_to_proc_term(&mut self) {
        // For safety, so that users do not accidentally crash vuit
        let command = self.typed_input.trim_start_matches(';').to_string();
        match command.as_str() {
            "vuit" => {
                self.term_out = "Nice Try".to_string();
            }
            "exit" => {
                self.restart_terminal_session();
                self.toggle_terminal = !self.toggle_terminal;
            }
            "quit" => {
                self.restart_terminal_session();
                self.toggle_terminal = !self.toggle_terminal;
            }
            "restart" => {
                self.restart_terminal_session();
            }
            "clear" => {
                self.restart_terminal_session();
            }
            _ => {
                if let Some(ref mut bash_stdin) = *self.command_sender.lock().unwrap() {
                    match writeln!(bash_stdin, "{}", command) {
                        _ => {}
                    };
                }
            }
        }
    }

    fn render_output(&self) -> String {
        // Fetch the output from PTY (this is simplified for the example)
        let output_str = {
            let output = self.process_out.lock().unwrap().clone();
            output.join("\n") // Join the lines together
        };
        output_str
    }

    fn ui(&mut self, frame: &mut Frame) {
        // Window Titles
        let search_title = if self.toggle_terminal {
            Line::from(" Command Line ")
        } else {
            Line::from(" Search ")
        };

        let preview_title = Line::from(" Preview ");
        let file_list_title = Line::from(" Files ");
        let recent_files_title = Line::from(" Recent ");

        // Window Blocks
        let search_block = Block::bordered()
            .title(search_title.left_aligned())
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

        // Defining toggle terminal line lengths
        let (search_lines, terminal_lines) = if self.toggle_terminal || self.toggle_help_menu {
            (SEARCH_BAR_NUM_LINES, TERMINAL_NUM_LINES)
        } else {
            (SEARCH_BAR_NUM_LINES, 0)
        };

        let content_lines = if frame
            .area()
            .height
            .checked_sub(search_lines + terminal_lines)
            > Some(0)
        {
            frame.area().height - search_lines - terminal_lines
        } else {
            frame.area().height
        };

        // Layout Description
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(content_lines),
                Constraint::Length(search_lines + terminal_lines),
            ])
            .split(frame.area());

        let top_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[0]);

        let top_chunks_left_length =
            if top_chunks[0].height.checked_sub(RECENT_BUFFERS_NUM_LINES) > Some(0) {
                top_chunks[0].height - RECENT_BUFFERS_NUM_LINES
            } else {
                top_chunks[0].height
            };

        let left_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(RECENT_BUFFERS_NUM_LINES),
                Constraint::Length(top_chunks_left_length),
            ])
            .split(top_chunks[0]);

        let search_terminal_chunks = if self.toggle_terminal || self.toggle_help_menu {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(terminal_lines),
                    Constraint::Length(search_lines),
                ])
                .split(chunks[1])
        } else {
            chunks
        };

        let help_text_box = List::new(vec![" Help -> <C-h>"])
            .block(Block::bordered().border_set(border::THICK))
            .style(
                Style::default()
                    .fg(grab_config_color(&self.config.colorscheme))
                    .add_modifier(Modifier::BOLD),
            );

        let search_split_help_length = if search_terminal_chunks[1]
            .height
            .checked_sub(HELP_TEXT_BOX_NUM_LINES)
            > Some(0)
        {
            search_terminal_chunks[0].width - HELP_TEXT_BOX_NUM_LINES
        } else {
            search_terminal_chunks[0].width
        };

        let search_split_help_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(search_split_help_length),
                Constraint::Length(HELP_TEXT_BOX_NUM_LINES),
            ])
            .split(search_terminal_chunks[1]);

        // Rendering of Windows
        frame.render_stateful_widget(recent_files_list, left_chunks[0], &mut self.recent_state);
        frame.render_stateful_widget(file_list_list, left_chunks[1], &mut self.file_list_state);
        frame.render_widget(preview_list, top_chunks[1]);
        frame.render_widget(search_para, search_split_help_chunks[0]);
        frame.render_stateful_widget(
            help_text_box,
            search_split_help_chunks[1],
            &mut self.help_menu_state,
        );

        if self.toggle_help_menu {
            self.help_menu = vec![
                "(General Commands)".to_string(),
                "<C-t>                  - Toggle terminal window".to_string(),
                "<C-h>                  - Toggle help menu window".to_string(),
                "<C-r>                  - Rescan CWD for updates".to_string(),
                "Esc                    - Exit Vuit".to_string(),
                "".to_string(),
                "(File List Focus Commands)".to_string(),
                "Up/Down, Ctrl-j/Ctrl-k - Navigate the file list".to_string(),
                "Enter                  - Open selected file".to_string(),
                "Tab                    - Switch between recent and file windows".to_string(),
                "".to_string(),
                "(Terminal Focus Commands)".to_string(),
                "<C-t>                  - Switches focus back to the file list, but terminal session is preserved".to_string(),
                "\"quit\", \"exit\"         - Switches focus back to the file list and restarts the terminal instance".to_string(), 
                "\"restart\"              - If terminal seems unresponsive, this will restart the session".to_string(),
            ];

            let help_para = List::new(self.help_menu.to_owned())
                .block(
                    Block::bordered()
                        .border_set(border::THICK)
                        .title(Line::from(" Help Menu ").centered()),
                )
                .style(Style::default().fg(Color::White));

            frame.render_widget(help_para, search_terminal_chunks[0]);
        } else if self.toggle_terminal {
            // Define terminal paragraph
            self.term_out.clear();
            self.term_out = self.render_output();
            let terminal_para = Paragraph::new(remove_ansi_escape_codes(&self.term_out))
                .block(
                    Block::bordered()
                        .border_set(border::THICK)
                        .title(Line::from(" Terminal ").centered()),
                )
                .style(Style::default().fg(Color::White));

            frame.render_widget(terminal_para, search_terminal_chunks[0]);
        }
    }

    fn handle_events(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                    if self.toggle_terminal {
                        self.handle_key_event_terminal_emu(key_event, terminal);
                    } else {
                        self.handle_key_event(key_event, terminal);
                    }
                }
                _ => {}
            };
        }
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

        let matches_c: Vec<_> = matches
            .into_iter()
            .map(|(_, s)| clean_utf8_content(s))
            .collect();

        matches_c.iter().map(|s| s.to_string()).collect()
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

        let num_lines = if self.toggle_terminal || self.toggle_help_menu {
            PREVIEW_NUM_LINES - TERMINAL_NUM_LINES
        } else {
            PREVIEW_NUM_LINES
        };

        let num_lines: usize = num_lines as usize;

        match File::open(file_path) {
            Ok(file) => {
                let reader = BufReader::new(file);
                reader
                    .lines()
                    .take(num_lines)
                    .filter_map(Result::ok)
                    .map(|line| clean_utf8_content(&line))
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

    fn handle_key_event_terminal_emu(
        &mut self,
        key_event: KeyEvent,
        terminal: &mut DefaultTerminal,
    ) {
        match key_event {
            KeyEvent {
                code: KeyCode::Char(c),
                modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT,
                ..
            } => {
                self.typed_input.push(c);
            }
            KeyEvent {
                code: KeyCode::Backspace,
                ..
            } => {
                if self.typed_input.is_empty() {
                    return;
                }

                self.typed_input.pop();
            }
            KeyEvent {
                code: KeyCode::Enter,
                ..
            } => {
                self.send_cmd_to_proc_term();
                self.typed_input.clear();
                self.process_out.lock().unwrap().clear();
            }
            KeyEvent {
                code: KeyCode::Esc, ..
            } => {
                // Exit when Esc is pressed
                self.exit = true;
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
            KeyEvent {
                code: KeyCode::Char('t'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.toggle_terminal = !self.toggle_terminal;
            }
            KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => self.restart_terminal_session(),
            KeyEvent {
                code: KeyCode::Char('h'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.toggle_help_menu = !self.toggle_help_menu;
            }
            _ => {}
        };
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

                if self.file_list.is_empty() {
                    return;
                }

                if self.switch_focus {
                    self.file_list_state.select(Some(self.hltd_file));
                    if self.hltd_file >= self.file_list.len() && !self.file_list.is_empty() {
                        self.hltd_file = self.file_list.len() - 1;
                    }
                    self.preview = self.run_preview_cmd();
                } else {
                    self.recent_state.select(Some(self.hltd_file));
                    if self.hltd_file >= self.recent_files.len() && !self.recent_files.is_empty() {
                        self.hltd_file = self.recent_files.len() - 1;
                    }
                }
            }
            KeyEvent {
                code: KeyCode::Backspace,
                ..
            } => {
                // FZF search after each backspace

                if self.typed_input.is_empty() {
                    return;
                }

                self.typed_input.pop();
                self.file_list = self.run_search_cmd();

                if self.file_list.is_empty() {
                    return;
                }

                if self.switch_focus {
                    self.file_list_state.select(Some(self.hltd_file));
                    if self.hltd_file >= self.file_list.len() && !self.file_list.is_empty() {
                        self.hltd_file = self.file_list.len() - 1;
                    }
                    self.preview = self.run_preview_cmd();
                } else {
                    self.recent_state.select(Some(self.hltd_file));
                    if self.hltd_file >= self.recent_files.len() && !self.recent_files.is_empty() {
                        self.hltd_file = self.recent_files.len() - 1;
                    }
                }
            }
            KeyEvent {
                code: KeyCode::Enter,
                ..
            } => {
                // Start editor on highlighted file when Enter is pressed
                if self.file_list.is_empty() {
                    return;
                }

                if self.switch_focus && !self.recent_files.contains(&self.file_list[self.hltd_file])
                {
                    self.recent_files
                        .push(self.file_list[self.hltd_file].to_owned());
                }

                if self.recent_files.len() > 5 {
                    self.recent_files.remove(0);
                }

                if self.switch_focus {
                    let _ = Command::new(&self.config.editor)
                        .arg(&self.file_list[self.hltd_file])
                        .status()
                        .expect("Failed to start selected editor");
                } else {
                    let _ = Command::new(&self.config.editor)
                        .arg(&self.recent_files[self.hltd_file])
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
                    if self.file_list.is_empty() {
                        return;
                    }
                } else if self.recent_files.is_empty() {
                    return;
                }

                self.hltd_file += 1;

                if self.switch_focus {
                    if self.hltd_file >= self.file_list.len() && !self.file_list.is_empty() {
                        self.hltd_file = self.file_list.len() - 1;
                    }
                    self.file_list_state.select(Some(self.hltd_file));
                    self.preview = self.run_preview_cmd();
                } else {
                    if self.hltd_file >= self.recent_files.len() && !self.recent_files.is_empty() {
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
                    if self.file_list.is_empty() {
                        return;
                    }
                } else if self.recent_files.is_empty() {
                    return;
                }

                if self.hltd_file == 0 {
                    return;
                }

                self.hltd_file -= 1;

                if self.switch_focus {
                    if self.hltd_file >= self.file_list.len() && !self.file_list.is_empty() {
                        self.hltd_file = self.file_list.len() - 1;
                    }
                    self.file_list_state.select(Some(self.hltd_file));
                    self.preview = self.run_preview_cmd();
                } else {
                    if self.hltd_file >= self.recent_files.len() && !self.recent_files.is_empty() {
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
                if self.recent_files.is_empty() {
                    return;
                }

                self.switch_focus = !self.switch_focus;

                if self.switch_focus {
                    self.recent_state.select(None);
                    self.hltd_file = 0;
                    self.file_list_state.select(Some(self.hltd_file));
                    if self.file_list.is_empty() {
                        return;
                    }
                    self.preview = self.run_preview_cmd();
                } else {
                    self.file_list_state.select(None);
                    self.hltd_file = 0;
                    self.recent_state.select(Some(self.hltd_file));
                    if self.recent_files.is_empty() {
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
            KeyEvent {
                code: KeyCode::Char('t'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.typed_input.clear();
                self.toggle_terminal = !self.toggle_terminal;
            }
            KeyEvent {
                code: KeyCode::Char('h'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.toggle_help_menu = !self.toggle_help_menu;
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

    let contents = fs::read_to_string(vuitrc_path).unwrap_or_default();

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
        config,
        ..Default::default()
    };

    let vuit_result = vuit_app.run(&mut terminal);
    ratatui::restore();
    vuit_result
}
