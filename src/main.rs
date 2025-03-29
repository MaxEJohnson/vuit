use std::io::prelude::*;
use std::io::{self, BufReader};
use std::process::{Command, Stdio};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use ratatui::{
    prelude::{Constraint, Direction, Layout},
    style::{Color, Style, Stylize},
    symbols::border,
    text::{Line, Text},
    widgets::{Block, List, ListState, Paragraph},
    DefaultTerminal, Frame,
};

use clap::Command as OtherCommand;

#[derive(Debug, Default)]
pub struct Vuit {
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
            .style(Style::default().fg(Color::LightBlue));

        let file_list_list = List::new(self.file_list.to_owned())
            .block(file_list_block)
            .style(Style::default().fg(Color::LightBlue))
            .highlight_style(Style::default().fg(Color::White).bg(Color::Blue));

        let preview_list = List::new(self.preview.to_owned())
            .block(preview_block)
            .style(Style::default().fg(Color::LightBlue));

        let recent_files_list = List::new(self.recent_files.to_owned())
            .block(recentfiles_block)
            .style(Style::default().fg(Color::LightBlue))
            .highlight_style(Style::default().fg(Color::White).bg(Color::Blue));

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

    fn run_fd_cmd(&mut self) -> () {
        let output = Command::new("fd").arg("--version").output();

        let search_cmd = match output {
            Ok(_output) => "fd",
            Err(_) => "fdfind",
        };

        let output = Command::new(search_cmd)
            .arg("-t")
            .arg("f")
            .stdout(Stdio::piped())
            .spawn()
            .expect("Failed to start fd");

        //self.fd_list = output.stdout;
        let stdout = output.stdout.expect("Failed to capture stdout");
        let reader = BufReader::new(stdout);

        self.fd_list = reader.lines().filter_map(Result::ok).collect();
    }

    fn run_search_cmd(&mut self) -> Vec<String> {
        let input = self.fd_list.join("\n");

        let mut fzf = Command::new("fzf")
            .arg("--filter")
            .arg(&self.typed_input)
            .stdin(Stdio::piped()) // Allow writing to stdin
            .stdout(Stdio::piped()) // Capture output
            .spawn()
            .expect("Failed to run fzf");

        if let Some(mut fzf_stdin) = fzf.stdin.take() {
            fzf_stdin
                .write_all(input.as_bytes())
                .expect("Failed to write to fzf stdin");
        }

        let output = fzf.wait_with_output().expect("Failed to read fzf output");

        let selected_files =
            std::str::from_utf8(&output.stdout).expect("Invalid UTF-8 output from fzf");

        selected_files
            .lines()
            .map(|line| line.to_string())
            .collect::<Vec<String>>()
    }

    fn run_preview_cmd(&mut self) -> Vec<String> {
        if self.file_list.len() == 0 {
            return vec![];
        }

        let output = if self.switch_focus {
            Command::new("head")
                .arg("-n")
                .arg("50")
                .arg(&self.file_list[self.hltd_file])
                .output()
        } else {
            Command::new("head")
                .arg("-n")
                .arg("50")
                .arg(&self.recent_files[self.hltd_file])
                .output()
        };

        match std::str::from_utf8(&output.unwrap().stdout) {
            Ok(output_str) => {
                // If the output is valid UTF-8, process the lines
                output_str
                    .lines()
                    .map(|line| line.to_string())
                    .collect::<Vec<String>>()
            }
            Err(_) => {
                // If the output is not valid UTF-8, return an empty vector
                "No Preview Available"
                    .split("\n")
                    .map(|line| line.to_string())
                    .collect::<Vec<String>>()
            }
        }
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
                // Start Vim on highlighted file when Enter is pressed
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
                    let _ = Command::new("vim")
                        .arg(self.file_list[self.hltd_file].to_owned())
                        .status()
                        .expect("Failed to start vim");
                } else {
                    let _ = Command::new("vim")
                        .arg(self.recent_files[self.hltd_file].to_owned())
                        .status()
                        .expect("Failed to start vim");
                }

                // Clear terminal on exit from Vim
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
            _ => {}
        };
    }
}

fn main() -> io::Result<()> {
    // Versioning
    let matches = OtherCommand::new("vuit")
        .version(env!("CARGO_PKG_VERSION")) // Uses the version from Cargo.toml
        .about("Vim User Interface Terminal - A Buffer Manager for Vim")
        .get_matches();

    if matches.contains_id("version") {
        println!("vuit version {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    // Vuit App Start
    let mut terminal = ratatui::init();
    let vuit_result = Vuit::default().run(&mut terminal);
    ratatui::restore();
    vuit_result
}
