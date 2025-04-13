use crate::vuit::ui::next_colorscheme;
use crate::vuit::utils::remove_ansi_escape_codes;
use crate::vuit::{Vuit, CONTEXT};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use portable_pty::{unix::UnixPtySystem, CommandBuilder, PtySize, PtySystem};
use ratatui::prelude::*;
use ratatui::{
    symbols::border,
    text::Line,
    widgets::{Block, Paragraph},
    DefaultTerminal, Frame,
};
use std::{
    io::{BufRead, BufReader, Write},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

pub fn render(app: &mut Vuit, frame: &mut Frame, chunks: &[Rect]) {
    app.term_out = render_output(app);
    let para = Paragraph::new(remove_ansi_escape_codes(&app.term_out))
        .block(
            Block::bordered()
                .title(Line::from(" Terminal ").centered())
                .border_set(border::ROUNDED),
        )
        .style(Style::default().fg(Color::White));

    frame.render_widget(para, chunks[0]);
}

pub fn handler(app: &mut Vuit, key: KeyEvent, terminal: &mut DefaultTerminal) {
    match key {
        KeyEvent {
            code: KeyCode::Char(c),
            modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT,
            ..
        } => {
            app.typed_input.push(c);
        }
        KeyEvent {
            code: KeyCode::Backspace,
            ..
        } => {
            if app.typed_input.is_empty() {
                return;
            }

            app.typed_input.pop();
        }
        KeyEvent {
            code: KeyCode::Enter,
            ..
        } => {
            send_cmd_to_proc_term(app);
            app.typed_input.clear();
            app.process_out.lock().unwrap().clear();
        }
        KeyEvent {
            code: KeyCode::Esc, ..
        } => {
            // exit when esc is pressed
            app.exit = true;
        }
        KeyEvent {
            code: KeyCode::Char('r'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => {
            // Refresh list of available files (e.g. after adding a new file, etc, ...)
            app.run_fd_cmd();
        }
        KeyEvent {
            code: KeyCode::Char('n'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => {
            next_colorscheme(app, terminal);
        }
        KeyEvent {
            code: KeyCode::Char('t'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => {
            app.prev_context = app.switch_context;
            app.switch_context = CONTEXT::FILEVIEWER;
        }
        KeyEvent {
            code: KeyCode::Char('c'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => {
            if let Some(ref mut bash_stdin) = *app.command_sender.lock().unwrap() {
                let _ = bash_stdin.write_all(&[0x003]);
            }
        }
        KeyEvent {
            code: KeyCode::Char('h'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => {
            if app.switch_context == CONTEXT::HELP {
                app.prev_context = CONTEXT::HELP;
                app.switch_context = app.prev_context;
            } else {
                app.prev_context = app.switch_context;
                app.switch_context = CONTEXT::HELP;
            }
        }
        _ => {}
    };
}

pub fn start_term(app: &mut Vuit) {
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
    let output = app.process_out.clone();

    thread::spawn(move || {
        for line in reader.lines() {
            if let Ok(line_str) = line {
                let mut output = output.lock().unwrap();
                output.push(line_str);
            }
        }
    });

    app.bash_process = Some(child);
    app.command_sender = Arc::new(Mutex::new(Some(Box::new(writer))));
}

fn restart_terminal_session(app: &mut Vuit) {
    if let Some(mut child) = app.bash_process.take() {
        child.kill().expect("Failed to kill bash process");
    }
    thread::sleep(Duration::from_millis(250));
    start_term(app);
}

fn send_cmd_to_proc_term(app: &mut Vuit) {
    // For safety, so that users do not accidentally crash vuit
    let command = app.typed_input.trim_start_matches(';').to_string();
    match command.as_str() {
        "vuit" => {
            app.term_out = "Nice Try".to_string();
        }
        "exit" => {
            restart_terminal_session(app);
            app.switch_context = CONTEXT::FILEVIEWER;
            app.prev_context = CONTEXT::TERMINAL;
        }
        "quit" => {
            restart_terminal_session(app);
            app.switch_context = CONTEXT::FILEVIEWER;
            app.prev_context = CONTEXT::TERMINAL;
        }
        "restart" => {
            restart_terminal_session(app);
        }
        "clear" => {
            restart_terminal_session(app);
        }
        _ => {
            if let Some(ref mut bash_stdin) = *app.command_sender.lock().unwrap() {
                match writeln!(bash_stdin, "{}", command) {
                    _ => {}
                };
            }
        }
    }
}

fn render_output(app: &Vuit) -> String {
    // Fetch the output from PTY (this is simplified for the example)
    let output_str = {
        let output = app.process_out.lock().unwrap().clone();
        output.join("\n") // Join the lines together
    };
    output_str
}
