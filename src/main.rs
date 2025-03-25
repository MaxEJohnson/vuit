use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};

use ratatui::{
    style::{Stylize, Style, Color},
    symbols::border,
    text::{Line, Text},
    widgets::{Block, Paragraph, List, ListState},
    prelude::{Constraint, Direction, Layout},
    DefaultTerminal, Frame,
};

use std::process::{Command, Stdio};

use clap::{Command as OtherCommand};

#[derive(Debug, Default)]
pub struct App {
    input: String,
    filelist: Vec<String>,
    preview: Vec<String>,
    recent_files: Vec<String>,
    list_state: ListState,
    highlightedfile: usize,
    exit: bool,
}

impl App {

    fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        while !self.exit {
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_events(terminal)?;
        }

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
            .style(Style::default().fg(Color::LightBlue));

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(95),
                Constraint::Percentage(5),
            ])
            .split(frame.area());

        let top_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(50),
                Constraint::Percentage(50),
            ])
            .split(chunks[0]);

        let left_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(30),
                Constraint::Percentage(70),
            ])
            .split(top_chunks[0]);

        frame.render_widget(recentfiles_list, left_chunks[0]);
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

        let output = Command::new("fdfind")
            .stdout(Stdio::piped()) 
            .spawn()
            .expect("Failed to start fdfind");

        let fzf_output = Command::new("fzf")
            .stdin(output.stdout.unwrap())
            .arg("--filter")
            .arg(search_term)
            .output()
            .expect("Failed to run fzf");

        let selected_files = std::str::from_utf8(&fzf_output.stdout)
            .expect("Invalid UTF-8 output from fzf");

        selected_files
            .lines()
            .map(|line| line.to_string())
            .collect::<Vec<String>>()
    }

    fn run_preview_cmd(&mut self, file: String) -> Vec<String> {
        let output = Command::new("cat")
            .arg(file)
            .output()
            .expect("Failed to run cat");

        std::str::from_utf8(&output.stdout)
            .expect("Invalid UTF-8 output from bat")
            .lines()
            .map(|line| line.to_string())
            .collect::<Vec<String>>()
    }
 
    fn handle_key_event(&mut self, key_event: KeyEvent, terminal: &mut DefaultTerminal) {
        match key_event.code {
            KeyCode::Char(c) => {
                self.input.push(c);
                self.filelist = self.run_search_cmd(self.input.clone());
                if self.filelist.len() == 0 {
                    return;
                }
                self.list_state.select(Some(self.highlightedfile));
                self.preview = self.run_preview_cmd(self.filelist[self.highlightedfile].clone());
            }
            KeyCode::Backspace => {
                self.input.pop();
                self.filelist = self.run_search_cmd(self.input.clone());
                if self.filelist.len() == 0 {
                    return;
                }
                self.list_state.select(Some(self.highlightedfile));
                self.preview = self.run_preview_cmd(self.filelist[self.highlightedfile].clone());
            }
            KeyCode::Enter => {
                if self.filelist.len() == 0 {
                    return;
                }
                self.recent_files.push(self.filelist[self.highlightedfile].clone());
                if self.recent_files.len() > 5 {
                    self.recent_files.remove(0);
                }

                let _ = Command::new("vim")
                    .arg(self.filelist[self.highlightedfile].clone())
                    .status()
                    .expect("Failed to start vim");

                let _ = terminal.clear();
                let _ = terminal.draw(|frame| self.draw(frame));
            }
            KeyCode::Esc => {
                self.exit = true;
            }
            KeyCode::Down => {
                if self.filelist.len() == 0 {
                    return;
                }
                self.highlightedfile += 1;
                if self.highlightedfile >= self.filelist.len() {
                    self.highlightedfile = self.filelist.len() - 1;
                }
                self.list_state.select(Some(self.highlightedfile));
                self.preview = self.run_preview_cmd(self.filelist[self.highlightedfile].clone());
            }
            KeyCode::Up => {
                if self.filelist.len() == 0 {
                    return;
                }
                if self.highlightedfile == 0 {
                    return;
                }
                self.highlightedfile -= 1;
                self.list_state.select(Some(self.highlightedfile));
                self.preview = self.run_preview_cmd(self.filelist[self.highlightedfile].clone());
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

