// Std Lib
use std::{
    fs::{self, File},
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

// Ratatui
use ratatui::{
    prelude::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols::border,
    text::{Line, Text},
    widgets::{Block, List, ListState, Paragraph},
    DefaultTerminal, Frame,
};

// External Crates
use clap::Command as ClapCommand;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use ignore::WalkBuilder;
use itertools::Itertools;
use memchr::memmem;
use portable_pty::{unix::UnixPtySystem, CommandBuilder, PtySize, PtySystem};
use rayon::prelude::*;
use regex::Regex;
use serde::{Deserialize, Serialize};

// Constants
const RECENT_BUFFERS_NUM_LINES: u16 = 8;
const TERMINAL_NUM_LINES: u16 = 20;
const SEARCH_BAR_NUM_LINES: u16 = 3;
const PREVIEW_NUM_LINES: u16 = 50;
const HELP_TEXT_BOX_NUM_LINES: u16 = 18;
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

// Focus States
#[derive(PartialEq, Eq)]
enum FOCUS {
    RECENTFILES,
    FILELIST,
    FILESTRLIST,
}
impl Default for FOCUS {
    fn default() -> Self {
        FOCUS::FILELIST
    }
}

// Helper Functions
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
fn expand_tilde(path: &str) -> PathBuf {
    if path.starts_with("~") {
        if let Some(home_dir) = dirs::home_dir() {
            return home_dir.join(&path[2..]); // Replace `~` with the home directory
        }
    }
    PathBuf::from(path)
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
    file_str_list: Vec<String>,
    preview: Vec<String>,
    recent_files: Vec<String>,
    fd_list: Vec<String>,
    term_out: String,
    help_menu: Vec<String>,
    current_filter: String,

    // Terminal vars
    bash_process: Option<Box<dyn portable_pty::Child + Send + Sync>>,
    process_out: Arc<Mutex<Vec<String>>>,
    command_sender: Arc<Mutex<Option<Box<dyn Write + Send>>>>,

    // State Variables
    switch_focus: FOCUS,
    toggle_terminal: bool,
    toggle_help_menu: bool,
    toggle_ss: bool,
    hltd_file: usize,
    file_list_state: ListState,
    file_str_list_state: ListState,
    recent_state: ListState,
    help_menu_state: ListState,

    // Termination
    exit: bool,
}

impl Vuit {
    fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        // Initialize Focus to File List
        self.switch_focus = FOCUS::FILELIST;
        self.toggle_terminal = false;
        self.toggle_help_menu = false;
        self.toggle_ss = false;

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
        if let Some(mut child) = self.bash_process.take() {
            child.kill().expect("Failed to kill bash process");
        }
        thread::sleep(Duration::from_secs(1));
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
        let (chunks, _content_lines) = self.make_main_layout(frame);
        let top_chunks = self.make_top_chunks(&chunks);
        let left_chunks = self.make_left_chunks(&top_chunks);
        let search_terminal_chunks = self.make_search_terminal_chunks(&chunks);
        let search_split_help_chunks = self.make_search_split_help_chunks(&search_terminal_chunks);

        self.render_recent_files(frame, &left_chunks);
        self.render_file_list(frame, &left_chunks);
        self.render_preview_list(frame, &top_chunks);
        self.render_search_input(frame, &search_split_help_chunks);
        self.render_help_toggle_text_box(frame, &search_split_help_chunks);

        if self.toggle_help_menu {
            self.render_help_menu(frame, &search_terminal_chunks);
        } else if self.toggle_terminal {
            self.render_terminal_output(frame, &search_terminal_chunks);
        } else if self.toggle_ss {
            self.render_file_string_list(frame, &search_terminal_chunks);
        } else {
            self.render_file_count_display(frame, &left_chunks);
        }
    }

    fn make_main_layout(&self, frame: &Frame) -> (Vec<Rect>, u16) {
        let (search_lines, terminal_lines) =
            if self.toggle_terminal || self.toggle_help_menu || self.toggle_ss {
                (SEARCH_BAR_NUM_LINES, TERMINAL_NUM_LINES)
            } else {
                (SEARCH_BAR_NUM_LINES, 0)
            };

        let content_lines = frame
            .area()
            .height
            .saturating_sub(search_lines + terminal_lines);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(content_lines),
                Constraint::Length(search_lines + terminal_lines),
            ])
            .split(frame.area());

        (chunks.to_vec(), content_lines)
    }

    fn make_top_chunks(&self, chunks: &[Rect]) -> Vec<Rect> {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[0])
            .to_vec()
    }

    fn make_left_chunks(&self, top_chunks: &[Rect]) -> Vec<Rect> {
        let left_height = top_chunks[0]
            .height
            .saturating_sub(RECENT_BUFFERS_NUM_LINES);

        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(RECENT_BUFFERS_NUM_LINES),
                Constraint::Length(left_height),
            ])
            .split(top_chunks[0])
            .to_vec()
    }

    fn make_search_terminal_chunks(&self, chunks: &[Rect]) -> Vec<Rect> {
        if self.toggle_terminal || self.toggle_help_menu || self.toggle_ss {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(TERMINAL_NUM_LINES),
                    Constraint::Length(SEARCH_BAR_NUM_LINES),
                ])
                .split(chunks[1])
                .to_vec()
        } else {
            chunks.to_vec()
        }
    }

    fn make_search_split_help_chunks(&self, search_terminal_chunks: &[Rect]) -> Vec<Rect> {
        let help_width = HELP_TEXT_BOX_NUM_LINES;
        let search_width = search_terminal_chunks[1].width.saturating_sub(help_width);

        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(search_width),
                Constraint::Length(help_width),
            ])
            .split(search_terminal_chunks[1])
            .to_vec()
    }

    fn render_recent_files(&mut self, f: &mut Frame, chunks: &[Rect]) {
        let block = Block::bordered()
            .title(Line::from(" Recent ").centered())
            .border_set(border::THICK);
        let list = List::new(self.recent_files.to_owned())
            .block(block)
            .style(Style::default().fg(grab_config_color(&self.config.colorscheme)))
            .highlight_style(
                Style::default()
                    .fg(Color::White)
                    .bg(grab_config_color(&self.config.highlight_color)),
            );
        f.render_stateful_widget(list, chunks[0], &mut self.recent_state);
    }

    fn render_file_list(&mut self, f: &mut Frame, chunks: &[Rect]) {
        let block = Block::bordered()
            .title(Line::from(" Files ").centered())
            .border_set(border::THICK);
        let list = List::new(self.file_list.to_owned())
            .block(block)
            .style(Style::default().fg(grab_config_color(&self.config.colorscheme)))
            .highlight_style(
                Style::default()
                    .fg(Color::White)
                    .bg(grab_config_color(&self.config.highlight_color)),
            );
        f.render_stateful_widget(list, chunks[1], &mut self.file_list_state);
    }

    fn render_preview_list(&self, f: &mut Frame, chunks: &[Rect]) {
        let block = Block::bordered()
            .title(Line::from(" Preview ").centered())
            .border_set(border::THICK);
        let list = List::new(self.preview.to_owned())
            .block(block)
            .style(Style::default().fg(grab_config_color(&self.config.colorscheme)));
        f.render_widget(list, chunks[1]);
    }

    fn render_search_input(&self, f: &mut Frame, chunks: &[Rect]) {
        let filter = if self.toggle_ss {
            let flt = if self.current_filter.is_empty() {
                "null".to_owned()
            } else {
                format!("\"{}\"", self.current_filter)
            };
            format!(" [FILE FILTER: {}] > {}", flt, self.typed_input)
        } else {
            format!(" > {}", self.typed_input)
        };

        let para = Paragraph::new(Text::from(filter))
            .block(
                Block::bordered()
                    .title(Line::from(" Command Line ").left_aligned())
                    .border_set(border::THICK),
            )
            .style(Style::default().fg(grab_config_color(&self.config.colorscheme)));

        f.render_widget(para, chunks[0]);
    }

    fn render_help_toggle_text_box(&mut self, f: &mut Frame, chunks: &[Rect]) {
        let box_widget = List::new(vec![" Help -> <C-h>"])
            .block(Block::bordered().border_set(border::THICK))
            .style(
                Style::default()
                    .fg(grab_config_color(&self.config.colorscheme))
                    .add_modifier(Modifier::BOLD),
            );
        f.render_stateful_widget(box_widget, chunks[1], &mut self.help_menu_state);
    }

    fn render_file_string_list(&mut self, f: &mut Frame, chunks: &[Rect]) {
        let block = Block::bordered()
            .title(Line::from(" Strings ").centered())
            .border_set(border::THICK);
        let list = List::new(self.file_str_list.to_owned())
            .block(block)
            .style(Style::default().fg(grab_config_color(&self.config.colorscheme)))
            .highlight_style(
                Style::default()
                    .fg(Color::White)
                    .bg(grab_config_color(&self.config.highlight_color)),
            );
        f.render_stateful_widget(list, chunks[0], &mut self.file_str_list_state);
    }

    fn render_terminal_output(&mut self, f: &mut Frame, chunks: &[Rect]) {
        self.term_out = self.render_output();
        let para = Paragraph::new(remove_ansi_escape_codes(&self.term_out))
            .block(
                Block::bordered()
                    .title(Line::from(" Terminal ").centered())
                    .border_set(border::THICK),
            )
            .style(Style::default().fg(Color::White));

        f.render_widget(para, chunks[0]);
    }

    fn render_help_menu(&mut self, f: &mut Frame, chunks: &[Rect]) {
        self.help_menu = self.build_help_text();
        let list = List::new(self.help_menu.to_owned())
            .block(
                Block::bordered()
                    .title(Line::from(" Help Menu ").centered())
                    .border_set(border::THICK),
            )
            .style(Style::default().fg(Color::White));
        f.render_widget(list, chunks[0]);
    }

    fn render_file_count_display(&self, f: &mut Frame, chunks: &[Rect]) {
        let count = format!(" [ {} / {} ] ", self.file_list.len(), self.fd_list.len());
        let para = Paragraph::new(count)
            .block(Block::bordered().border_set(border::THICK))
            .alignment(ratatui::prelude::Alignment::Center)
            .style(Style::default().fg(grab_config_color(&self.config.colorscheme)));

        let filecount_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(chunks[1].height.saturating_sub(4)),
                Constraint::Length(3),
            ])
            .split(chunks[1]);

        let right_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(chunks[1].width.saturating_sub(24)),
                Constraint::Length(21),
            ])
            .split(filecount_chunks[1]);

        f.render_widget(para, right_chunks[1]);
    }

    fn build_help_text(&self) -> Vec<String> {
        vec![
            "(General Commands)".into(),
            "<C-t> - Toggle terminal window".into(),
            "<C-h> - Toggle help menu window".into(),
            "<C-r> - Rescan CWD for updates".into(),
            "Esc   - Exit Vuit".into(),
            "".into(),
            "(File List Focus Commands)".into(),
            "Up/Down, Ctrl-j/Ctrl-k - Navigate the file list".into(),
            "Enter - Open selected file".into(),
            "Tab   - Switch between recent and file windows".into(),
            "".into(),
            "(Terminal Focus Commands)".into(),
            "<C-t> - Switches focus back to the file list, but terminal session is preserved"
                .into(),
            "quit, exit - Switches focus back to the file list and restarts the terminal instance"
                .into(),
            "restart - If terminal seems unresponsive, this will restart the session".into(),
        ]
    }

    fn handle_events(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        if !event::poll(Duration::from_millis(100))? {
            return Ok(());
        }

        if let Event::Key(key_event) = event::read()? {
            if key_event.kind != KeyEventKind::Press {
                return Ok(());
            }

            match (self.toggle_terminal, self.toggle_ss) {
                (true, _) => self.handle_key_event_terminal_emu(key_event, terminal),
                (false, true) => self.handle_key_event_rg(key_event, terminal),
                _ => self.handle_key_event(key_event, terminal),
            }
        }

        Ok(())
    }

    fn run_fd_cmd(&mut self) {
        self.fd_list = WalkBuilder::new(".")
            .standard_filters(true)
            .build()
            .filter_map(Result::ok)
            .map(|entry| entry.path().to_path_buf())
            .filter(|path| path.is_file())
            .filter_map(|path| path.to_str().map(String::from))
            .collect();
    }

    fn run_search_cmd(&mut self) -> Vec<String> {
        let matcher = SkimMatcherV2::default();

        self.fd_list
            .iter()
            .filter_map(|item| {
                matcher
                    .fuzzy_match(item, &self.typed_input)
                    .map(|score| (score, item))
            })
            .sorted_unstable_by(|a, b| b.0.cmp(&a.0))
            .map(|(_, s)| clean_utf8_content(s).to_string())
            .collect()
    }

    fn search_filelist_str(&mut self) -> Vec<String> {
        let search = self.typed_input.to_lowercase();

        self.file_list
            .par_iter()
            .filter_map(|path_str| {
                let path = Path::new(path_str);
                let file = File::open(path).ok()?;
                let reader = BufReader::new(file);

                for (line_number, line) in reader.lines().enumerate() {
                    let line = line.ok()?;
                    let haystack = line.to_lowercase();
                    if memmem::find(haystack.as_bytes(), search.as_bytes()).is_some() {
                        return Some(clean_utf8_content(&format!(
                            "{}:{}:{}",
                            path.display(),
                            line_number + 1,
                            line
                        )));
                    }
                }

                None
            })
            .collect()
    }

    fn run_preview_cmd(&mut self) -> Vec<String> {
        let file_list = match self.switch_focus {
            FOCUS::RECENTFILES => &self.recent_files,
            FOCUS::FILELIST => &self.file_list,
            FOCUS::FILESTRLIST => &self.file_str_list,
        };

        if file_list.is_empty() || self.switch_focus == FOCUS::FILESTRLIST {
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
                if self.switch_focus == FOCUS::FILESTRLIST {
                    vec![]
                } else {
                    let reader = BufReader::new(file);
                    return reader
                        .lines()
                        .take(num_lines)
                        .filter_map(Result::ok)
                        .map(|line| clean_utf8_content(&line))
                        .collect::<Vec<String>>();
                }
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

    fn handle_key_event_rg(&mut self, key_event: KeyEvent, terminal: &mut DefaultTerminal) {
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
                if self.switch_focus == FOCUS::FILESTRLIST
                    && self.file_str_list_state.selected().is_some()
                {
                    if !self
                        .recent_files
                        .contains(&self.file_str_list[self.hltd_file])
                    {
                        let file_path = &self.file_str_list[self.hltd_file]
                            .split_once(':')
                            .map(|(before, _)| before)
                            .unwrap_or(self.file_str_list[self.hltd_file].as_str());
                        self.recent_files.push(file_path.to_string());
                    }

                    if self.recent_files.len() > 5 {
                        self.recent_files.remove(0);
                    }

                    let file_path = &self.file_str_list[self.hltd_file]
                        .split_once(':')
                        .map(|(before, _)| before)
                        .unwrap_or(self.file_str_list[self.hltd_file].as_str());

                    let linearg = if self.config.editor == "vim" {
                        let linenumnstr = self.file_str_list[self.hltd_file]
                            .split_once(':')
                            .map(|(_, after)| after)
                            .unwrap_or(self.file_str_list[self.hltd_file].as_str());
                        let linenum = linenumnstr
                            .split_once(':')
                            .map(|(before, _)| before)
                            .unwrap_or(linenumnstr);

                        format!("+{}", linenum)
                    } else {
                        String::new()
                    };

                    let _ = Command::new(&self.config.editor)
                        .arg(linearg)
                        .arg(file_path)
                        .status()
                        .expect("Failed to start selected editor");

                    self.file_str_list_state.select(None);
                    // Clear terminal on exit from editor
                    let _ = terminal.clear();
                    let _ = terminal.draw(|frame| self.ui(frame));
                } else {
                    self.file_str_list = self.search_filelist_str();
                }
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
                match self.switch_focus {
                    FOCUS::RECENTFILES => {
                        if self.recent_files.is_empty() {
                            return;
                        }
                    }
                    FOCUS::FILELIST => {
                        if self.file_list.is_empty() {
                            return;
                        }
                    }
                    FOCUS::FILESTRLIST => {
                        if self.file_str_list.is_empty() {
                            return;
                        }
                    }
                }

                self.hltd_file += 1;

                match self.switch_focus {
                    FOCUS::RECENTFILES => {
                        if self.hltd_file >= self.recent_files.len()
                            && !self.recent_files.is_empty()
                        {
                            self.hltd_file = self.recent_files.len() - 1;
                        }
                        self.recent_state.select(Some(self.hltd_file));
                    }
                    FOCUS::FILELIST => {
                        if self.hltd_file >= self.file_list.len() && !self.file_list.is_empty() {
                            self.hltd_file = self.file_list.len() - 1;
                        }
                        self.file_list_state.select(Some(self.hltd_file));
                    }
                    FOCUS::FILESTRLIST => {
                        if self.hltd_file >= self.file_str_list.len()
                            && !self.file_str_list.is_empty()
                        {
                            self.hltd_file = self.file_str_list.len() - 1;
                        }
                        self.file_str_list_state.select(Some(self.hltd_file));
                    }
                }
                self.preview = self.run_preview_cmd();
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
                match self.switch_focus {
                    FOCUS::RECENTFILES => {
                        if self.recent_files.is_empty() {
                            return;
                        }
                    }
                    FOCUS::FILELIST => {
                        if self.file_list.is_empty() {
                            return;
                        }
                    }
                    FOCUS::FILESTRLIST => {
                        if self.file_str_list.is_empty() {
                            return;
                        }
                    }
                }

                if self.hltd_file == 0 {
                    return;
                }

                self.hltd_file -= 1;
                match self.switch_focus {
                    FOCUS::RECENTFILES => {
                        self.recent_state.select(Some(self.hltd_file));
                    }
                    FOCUS::FILELIST => {
                        self.file_list_state.select(Some(self.hltd_file));
                    }
                    FOCUS::FILESTRLIST => {
                        self.file_str_list_state.select(Some(self.hltd_file));
                    }
                }
                self.preview = self.run_preview_cmd();
            }
            KeyEvent {
                code: KeyCode::Tab, ..
            } => {
                // Switch between recent and search files
                match self.switch_focus {
                    FOCUS::RECENTFILES => {
                        if !self.file_str_list.is_empty() {
                            self.switch_focus = FOCUS::FILESTRLIST;
                        }
                        if !self.file_list.is_empty() {
                            self.switch_focus = FOCUS::FILELIST;
                        }
                    }
                    FOCUS::FILELIST => {
                        if !self.recent_files.is_empty() {
                            self.switch_focus = FOCUS::RECENTFILES;
                        }

                        if !self.file_str_list.is_empty() {
                            self.switch_focus = FOCUS::FILESTRLIST;
                        }
                    }
                    FOCUS::FILESTRLIST => {
                        if !self.file_list.is_empty() {
                            self.switch_focus = FOCUS::FILELIST;
                        }
                        if !self.recent_files.is_empty() {
                            self.switch_focus = FOCUS::RECENTFILES;
                        }
                    }
                }

                match self.switch_focus {
                    FOCUS::RECENTFILES => {
                        self.file_list_state.select(None);
                        self.file_str_list_state.select(None);
                        self.hltd_file = 0;
                        self.recent_state.select(Some(self.hltd_file));
                        if self.recent_files.is_empty() {
                            return;
                        }
                    }
                    FOCUS::FILELIST => {
                        self.file_str_list_state.select(None);
                        self.recent_state.select(None);
                        self.hltd_file = 0;
                        self.file_list_state.select(Some(self.hltd_file));
                        if self.file_list.is_empty() {
                            return;
                        }
                    }
                    FOCUS::FILESTRLIST => {
                        self.file_list_state.select(None);
                        self.recent_state.select(None);
                        self.hltd_file = 0;
                        self.file_str_list_state.select(Some(self.hltd_file));
                        if self.file_str_list.is_empty() {
                            return;
                        }
                    }
                }
                self.preview = self.run_preview_cmd();
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
                code: KeyCode::Char('f'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.typed_input.clear();
                self.file_str_list.clear();
                self.toggle_ss = !self.toggle_ss;
                self.file_list = self.run_search_cmd();
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
                // exit when esc is pressed
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
            } => {
                if let Some(ref mut bash_stdin) = *self.command_sender.lock().unwrap() {
                    let _ = bash_stdin.write_all(&[0x003]);
                }
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

                match self.switch_focus {
                    FOCUS::RECENTFILES => {
                        self.recent_state.select(Some(self.hltd_file));
                        if self.hltd_file >= self.recent_files.len()
                            && !self.recent_files.is_empty()
                        {
                            self.hltd_file = self.recent_files.len() - 1;
                        }
                    }
                    FOCUS::FILELIST => {
                        self.file_list_state.select(Some(self.hltd_file));
                        if self.hltd_file >= self.file_list.len() && !self.file_list.is_empty() {
                            self.hltd_file = self.file_list.len() - 1;
                        }
                    }
                    FOCUS::FILESTRLIST => {
                        self.file_str_list_state.select(Some(self.hltd_file));
                        if self.hltd_file >= self.file_str_list.len()
                            && !self.file_str_list.is_empty()
                        {
                            self.hltd_file = self.file_str_list.len() - 1;
                        }
                    }
                }
                self.preview = self.run_preview_cmd();
            }
            KeyEvent {
                code: KeyCode::Backspace,
                ..
            } => {
                // FZF search after each Backspace

                if self.typed_input.is_empty() {
                    return;
                }

                self.typed_input.pop();
                self.file_list = self.run_search_cmd();

                match self.switch_focus {
                    FOCUS::RECENTFILES => {
                        self.recent_state.select(Some(self.hltd_file));
                        if self.hltd_file >= self.recent_files.len()
                            && !self.recent_files.is_empty()
                        {
                            self.hltd_file = self.recent_files.len() - 1;
                        }
                    }
                    FOCUS::FILELIST => {
                        self.file_list_state.select(Some(self.hltd_file));
                        if self.hltd_file >= self.file_list.len() && !self.file_list.is_empty() {
                            self.hltd_file = self.file_list.len() - 1;
                        }
                    }
                    FOCUS::FILESTRLIST => {
                        self.file_str_list_state.select(Some(self.hltd_file));
                        if self.hltd_file >= self.file_str_list.len()
                            && !self.file_str_list.is_empty()
                        {
                            self.hltd_file = self.file_str_list.len() - 1;
                        }
                    }
                }
                self.preview = self.run_preview_cmd();
            }
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                match self.switch_focus {
                    FOCUS::RECENTFILES => {
                        if self.hltd_file >= self.recent_files.len() {
                            return;
                        }
                        let _ = Command::new(&self.config.editor)
                            .arg(&self.recent_files[self.hltd_file])
                            .status()
                            .expect("Failed to start selected editor");
                    }
                    FOCUS::FILELIST => {
                        if self.hltd_file >= self.file_list.len() {
                            return;
                        }
                        let _ = Command::new(&self.config.editor)
                            .arg(&self.file_list[self.hltd_file])
                            .status()
                            .expect("Failed to start selected editor");
                    }
                    FOCUS::FILESTRLIST => {
                        if self.hltd_file >= self.file_str_list.len() {
                            return;
                        }
                        let file_path = &self.file_str_list[self.hltd_file]
                            .split_once(':')
                            .map(|(before, _)| before)
                            .unwrap_or(self.file_str_list[self.hltd_file].as_str());
                        let _ = Command::new(&self.config.editor)
                            .arg(file_path)
                            .status()
                            .expect("Failed to start selected editor");
                    }
                }

                if self.switch_focus == FOCUS::FILELIST
                    && !self.recent_files.contains(&self.file_list[self.hltd_file])
                {
                    self.recent_files
                        .push(self.file_list[self.hltd_file].to_owned());
                }

                if self.recent_files.len() > 5 {
                    self.recent_files.remove(0);
                }

                // Clear terminal on exit from editor
                let _ = terminal.clear();
                let _ = terminal.draw(|frame| self.ui(frame));
            }
            KeyEvent {
                code: KeyCode::Char('f'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.current_filter = self.typed_input.clone();
                self.typed_input.clear();
                self.toggle_ss = !self.toggle_ss;
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
                match self.switch_focus {
                    FOCUS::RECENTFILES => {
                        if self.recent_files.is_empty() {
                            return;
                        }
                    }
                    FOCUS::FILELIST => {
                        if self.file_list.is_empty() {
                            return;
                        }
                    }
                    FOCUS::FILESTRLIST => {
                        if self.file_str_list.is_empty() {
                            return;
                        }
                    }
                }

                self.hltd_file += 1;

                match self.switch_focus {
                    FOCUS::RECENTFILES => {
                        if self.hltd_file >= self.recent_files.len()
                            && !self.recent_files.is_empty()
                        {
                            self.hltd_file = self.recent_files.len() - 1;
                        }
                        self.recent_state.select(Some(self.hltd_file));
                    }
                    FOCUS::FILELIST => {
                        if self.hltd_file >= self.file_list.len() && !self.file_list.is_empty() {
                            self.hltd_file = self.file_list.len() - 1;
                        }
                        self.file_list_state.select(Some(self.hltd_file));
                    }
                    FOCUS::FILESTRLIST => {
                        if self.hltd_file >= self.file_str_list.len()
                            && !self.file_str_list.is_empty()
                        {
                            self.hltd_file = self.file_str_list.len() - 1;
                        }
                        self.file_str_list_state.select(Some(self.hltd_file));
                    }
                }
                self.preview = self.run_preview_cmd();
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
                match self.switch_focus {
                    FOCUS::RECENTFILES => {
                        if self.recent_files.is_empty() {
                            return;
                        }
                    }
                    FOCUS::FILELIST => {
                        if self.file_list.is_empty() {
                            return;
                        }
                    }
                    FOCUS::FILESTRLIST => {
                        if self.file_str_list.is_empty() {
                            return;
                        }
                    }
                }

                if self.hltd_file == 0 {
                    return;
                }

                self.hltd_file -= 1;
                match self.switch_focus {
                    FOCUS::RECENTFILES => {
                        self.recent_state.select(Some(self.hltd_file));
                    }
                    FOCUS::FILELIST => {
                        self.file_list_state.select(Some(self.hltd_file));
                    }
                    FOCUS::FILESTRLIST => {
                        self.file_str_list_state.select(Some(self.hltd_file));
                    }
                }
                self.preview = self.run_preview_cmd();
            }
            KeyEvent {
                code: KeyCode::Tab, ..
            } => {
                // Switch between recent and search files
                match self.switch_focus {
                    FOCUS::RECENTFILES => {
                        if !self.file_str_list.is_empty() {
                            self.switch_focus = FOCUS::FILESTRLIST;
                        }
                        if !self.file_list.is_empty() {
                            self.switch_focus = FOCUS::FILELIST;
                        }
                    }
                    FOCUS::FILELIST => {
                        if !self.recent_files.is_empty() {
                            self.switch_focus = FOCUS::RECENTFILES;
                        }

                        if !self.file_str_list.is_empty() {
                            self.switch_focus = FOCUS::FILESTRLIST;
                        }
                    }
                    FOCUS::FILESTRLIST => {
                        if !self.file_list.is_empty() {
                            self.switch_focus = FOCUS::FILELIST;
                        }
                        if !self.recent_files.is_empty() {
                            self.switch_focus = FOCUS::RECENTFILES;
                        }
                    }
                }

                match self.switch_focus {
                    FOCUS::RECENTFILES => {
                        self.file_list_state.select(None);
                        self.file_str_list_state.select(None);
                        self.hltd_file = 0;
                        self.recent_state.select(Some(self.hltd_file));
                        if self.recent_files.is_empty() {
                            return;
                        }
                    }
                    FOCUS::FILELIST => {
                        self.file_str_list_state.select(None);
                        self.recent_state.select(None);
                        self.hltd_file = 0;
                        self.file_list_state.select(Some(self.hltd_file));
                        if self.file_list.is_empty() {
                            return;
                        }
                    }
                    FOCUS::FILESTRLIST => {
                        self.file_list_state.select(None);
                        self.recent_state.select(None);
                        self.hltd_file = 0;
                        self.file_str_list_state.select(Some(self.hltd_file));
                        if self.file_str_list.is_empty() {
                            return;
                        }
                    }
                }
                self.preview = self.run_preview_cmd();
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
pub struct VuitRC {
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

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
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

    if let Err(e) = vuit_result {
        Err(e.into())
    } else {
        Ok(())
    }
}
