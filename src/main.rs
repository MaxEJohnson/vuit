use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use ratatui::{
    prelude::{Constraint, Direction, Layout},
    style::{Color, Style, Stylize},
    symbols::border,
    text::{Line, Text},
    widgets::{Block, List, ListState, Paragraph},
    DefaultTerminal, Frame,
};

use std::process::{Command, Stdio};

use clap::Command as OtherCommand;

#[derive(Debug, Default)]
pub struct App {
    input: String,
    filelist: Vec<String>,
    preview: Vec<String>,
    recent_files: Vec<String>,
    list_state: ListState,
    recent_state: ListState,
    switch_focus: bool,
    highlightedfile: usize,
    exit: bool,
}

impl App {
    fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        self.switch_focus = true;
        self.filelist = self.run_search_cmd(self.input.clone());
        self.list_state.select(Some(self.highlightedfile));
        if self.highlightedfile >= self.filelist.len() && self.filelist.len() > 0 {
            self.highlightedfile = self.filelist.len() - 1;
        }
        self.preview = self.run_preview_cmd(self.filelist[self.highlightedfile].clone());

        while !self.exit {
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_events(terminal)?;
        }
        let _ = terminal.clear();

        Ok(())
    }

    fn draw(&mut self, frame: &mut Frame) {
        let search_title = Line::from(" Search ".underlined());
        let preview_title = Line::from(" Preview ".underlined());
        let filelist_title = Line::from(" Files ".underlined());
        let recentfiles_title = Line::from(" Recent ".underlined());

        let search_block = Block::bordered()
            .title(search_title.centered())
            .border_set(border::THICK);

        let preview_block = Block::bordered()
            .title(preview_title.centered())
            .border_set(border::THICK);

        let filelist_block = Block::bordered()
            .title(filelist_title.centered())
            .border_set(border::THICK);

        let recentfiles_block = Block::bordered()
            .title(recentfiles_title.centered())
            .border_set(border::THICK);

        let input = Text::from("> ".to_owned() + &self.input.clone());

        let search_para = Paragraph::new(input.clone())
            .block(search_block)
            .style(Style::default().fg(Color::LightBlue));

        let filelist_list = List::new(self.filelist.clone())
            .block(filelist_block)
            .style(Style::default().fg(Color::LightBlue))
            .highlight_style(Style::default().fg(Color::White).bg(Color::Blue));

        //let styled_preview = self.preview.iter().map(|line| Text::from(line.clone()).blue()).collect::<Vec<Text>>();

        //let preview_list = List::new(styled_preview.clone())
        let preview_list = List::new(self.preview.clone())
            .block(preview_block)
            .style(Style::default().fg(Color::LightBlue));

        let recentfiles_list = List::new(self.recent_files.clone())
            .block(recentfiles_block)
            .style(Style::default().fg(Color::LightBlue))
            .highlight_style(Style::default().fg(Color::White).bg(Color::Blue));

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

        frame.render_stateful_widget(recentfiles_list, left_chunks[0], &mut self.recent_state);
        frame.render_stateful_widget(filelist_list, left_chunks[1], &mut self.list_state);
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

    fn run_search_cmd(&mut self, search_term: String) -> Vec<String> {
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

        let fzf_output = Command::new("fzf")
            .stdin(output.stdout.unwrap())
            .arg("--filter")
            .arg(search_term)
            .output()
            .expect("Failed to run fzf");

        let selected_files =
            std::str::from_utf8(&fzf_output.stdout).expect("Invalid UTF-8 output from fzf");

        selected_files
            .lines()
            .map(|line| line.to_string())
            .collect::<Vec<String>>()
    }

    fn run_preview_cmd(&mut self, file: String) -> Vec<String> {
        if self.filelist.len() == 0 {
            return vec![];
        }
        let output = Command::new("cat")
            .arg(file)
            .output()
            .expect("Failed to run cat");

        match std::str::from_utf8(&output.stdout) {
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
        match key_event {
            KeyEvent {
                code: KeyCode::Char(c),
                modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT,
                ..
            } => {
                self.input.push(c);
                self.filelist = self.run_search_cmd(self.input.clone());
                if self.filelist.len() == 0 {
                    return;
                }
                if self.switch_focus {
                    self.list_state.select(Some(self.highlightedfile));
                    if self.highlightedfile >= self.filelist.len() && self.filelist.len() > 0 {
                        self.highlightedfile = self.filelist.len() - 1;
                    }
                    self.preview =
                        self.run_preview_cmd(self.filelist[self.highlightedfile].clone());
                } else {
                    self.recent_state.select(Some(self.highlightedfile));
                    if self.highlightedfile >= self.recent_files.len()
                        && self.recent_files.len() > 0
                    {
                        self.highlightedfile = self.recent_files.len() - 1;
                    }
                }
            }
            KeyEvent {
                code: KeyCode::Backspace,
                ..
            } => {
                if self.input.len() == 0 {
                    return;
                }
                self.input.pop();
                self.filelist = self.run_search_cmd(self.input.clone());
                if self.filelist.len() == 0 {
                    return;
                }
                if self.switch_focus {
                    self.list_state.select(Some(self.highlightedfile));
                    if self.highlightedfile >= self.filelist.len() && self.filelist.len() > 0 {
                        self.highlightedfile = self.filelist.len() - 1;
                    }
                    self.preview =
                        self.run_preview_cmd(self.filelist[self.highlightedfile].clone());
                } else {
                    self.recent_state.select(Some(self.highlightedfile));
                    if self.highlightedfile >= self.recent_files.len()
                        && self.recent_files.len() > 0
                    {
                        self.highlightedfile = self.recent_files.len() - 1;
                    }
                }
            }
            KeyEvent {
                code: KeyCode::Enter,
                ..
            } => {
                if self.filelist.len() == 0 {
                    return;
                }
                if self.switch_focus {
                    if !self
                        .recent_files
                        .contains(&self.filelist[self.highlightedfile])
                    {
                        self.recent_files
                            .push(self.filelist[self.highlightedfile].clone());
                    }
                }
                if self.recent_files.len() > 5 {
                    self.recent_files.remove(0);
                }

                if self.switch_focus {
                    let _ = Command::new("vim")
                        .arg(self.filelist[self.highlightedfile].clone())
                        .status()
                        .expect("Failed to start vim");
                } else {
                    let _ = Command::new("vim")
                        .arg(self.recent_files[self.highlightedfile].clone())
                        .status()
                        .expect("Failed to start vim");
                }

                let _ = terminal.clear();
                let _ = terminal.draw(|frame| self.draw(frame));
            }
            KeyEvent {
                code: KeyCode::Esc, ..
            } => {
                self.exit = true;
            }
            KeyEvent {
                code: KeyCode::Char('j'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                if self.switch_focus {
                    if self.filelist.len() == 0 {
                        return;
                    }
                } else {
                    if self.recent_files.len() == 0 {
                        return;
                    }
                }
                self.highlightedfile += 1;
                if self.switch_focus {
                    if self.highlightedfile >= self.filelist.len() && self.filelist.len() > 0 {
                        self.highlightedfile = self.filelist.len() - 1;
                    }
                    self.list_state.select(Some(self.highlightedfile));
                    self.preview =
                        self.run_preview_cmd(self.filelist[self.highlightedfile].clone());
                } else {
                    if self.highlightedfile >= self.recent_files.len()
                        && self.recent_files.len() > 0
                    {
                        self.highlightedfile = self.recent_files.len() - 1;
                    }
                    self.recent_state.select(Some(self.highlightedfile));
                    self.preview =
                        self.run_preview_cmd(self.recent_files[self.highlightedfile].clone());
                }
            }
            KeyEvent {
                code: KeyCode::Char('k'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                if self.switch_focus {
                    if self.filelist.len() == 0 {
                        return;
                    }
                } else {
                    if self.recent_files.len() == 0 {
                        return;
                    }
                }
                if self.highlightedfile == 0 {
                    return;
                }
                self.highlightedfile -= 1;
                if self.switch_focus {
                    if self.highlightedfile >= self.filelist.len() && self.filelist.len() > 0 {
                        self.highlightedfile = self.filelist.len() - 1;
                    }
                    self.list_state.select(Some(self.highlightedfile));
                    self.preview =
                        self.run_preview_cmd(self.filelist[self.highlightedfile].clone());
                } else {
                    if self.highlightedfile >= self.recent_files.len()
                        && self.recent_files.len() > 0
                    {
                        self.highlightedfile = self.recent_files.len() - 1;
                    }
                    self.recent_state.select(Some(self.highlightedfile));
                    self.preview =
                        self.run_preview_cmd(self.recent_files[self.highlightedfile].clone());
                }
            }
            KeyEvent {
                code: KeyCode::Tab, ..
            } => {
                if self.recent_files.len() == 0 {
                    return;
                }
                self.switch_focus = !self.switch_focus;
                if self.switch_focus {
                    self.recent_state.select(None);
                    self.highlightedfile = 0;
                    self.list_state.select(Some(self.highlightedfile));
                    if self.filelist.len() == 0 {
                        return;
                    }
                    self.preview =
                        self.run_preview_cmd(self.filelist[self.highlightedfile].clone());
                } else {
                    self.list_state.select(None);
                    self.highlightedfile = 0;
                    self.recent_state.select(Some(self.highlightedfile));
                    if self.recent_files.len() == 0 {
                        return;
                    }
                    self.preview =
                        self.run_preview_cmd(self.recent_files[self.highlightedfile].clone());
                }
            }
            _ => {}
        };
    }
}

fn main() -> io::Result<()> {
    let matches = OtherCommand::new("vuit")
        .version(env!("CARGO_PKG_VERSION")) // Uses the version from Cargo.toml
        .about("Vim User Interface Terminal - A Buffer Manager for Vim")
        .get_matches();

    if matches.contains_id("version") {
        println!("vuit version {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let mut terminal = ratatui::init();
    let app_result = App::default().run(&mut terminal);
    ratatui::restore();
    app_result
}
