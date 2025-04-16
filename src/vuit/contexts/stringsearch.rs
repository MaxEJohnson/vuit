use crate::vuit::ui::{dispatch_render, next_colorscheme};
use crate::vuit::utils::grab_config_color;
use crate::vuit::{Context, Focus, Vuit};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::prelude::*;
use ratatui::{
    symbols::border,
    text::Line,
    widgets::{Block, List},
    DefaultTerminal, Frame,
};
use std::process::Command;

pub fn render(app: &mut Vuit, frame: &mut Frame, chunks: &[Rect]) {
    let area_height = chunks[0].height as usize;
    let total = app.file_str_list.len();
    let selected = if Focus::Filestrlist == app.switch_focus {
        app.hltd_file.min(total.saturating_sub(1))
    } else {
        0
    };

    // Compute visible range based on hltd_file
    let start = if selected >= area_height {
        selected + 1 - area_height
    } else {
        0
    };
    let end = (start + area_height).min(total);
    let visible = &app.file_str_list[start..end];

    // Only set selection if this context is focused
    if app.switch_focus == Focus::Filestrlist {
        app.file_str_list_state.select(Some(selected - start));
    }

    let block = Block::bordered()
        .title(Line::from(" Strings ").centered())
        .border_set(border::ROUNDED);

    let list = List::new(visible.to_owned())
        .block(block)
        .style(Style::default().fg(grab_config_color(&app.config.colorscheme)))
        .highlight_style(
            Style::default()
                .fg(Color::White)
                .bg(grab_config_color(&app.config.highlight_color)),
        );

    frame.render_stateful_widget(list, chunks[0], &mut app.file_str_list_state);
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
            if app.switch_focus == Focus::Filestrlist
                && app.file_str_list_state.selected().is_some()
            {
                if !app.recent_files.contains(&app.file_str_list[app.hltd_file]) {
                    let file_path = &app.file_str_list[app.hltd_file]
                        .split_once(':')
                        .map(|(before, _)| before)
                        .unwrap_or(app.file_str_list[app.hltd_file].as_str());
                    app.recent_files.push(file_path.to_string());
                }

                if app.recent_files.len() > 5 {
                    app.recent_files.remove(0);
                }

                let file_path = &app.file_str_list[app.hltd_file]
                    .split_once(':')
                    .map(|(before, _)| before)
                    .unwrap_or(app.file_str_list[app.hltd_file].as_str());

                let linearg = if app.config.editor == "vim" {
                    let linenumnstr = app.file_str_list[app.hltd_file]
                        .split_once(':')
                        .map(|(_, after)| after)
                        .unwrap_or(app.file_str_list[app.hltd_file].as_str());
                    let linenum = linenumnstr
                        .split_once(':')
                        .map(|(before, _)| before)
                        .unwrap_or(linenumnstr);

                    format!("+{}", linenum)
                } else {
                    String::new()
                };

                let _ = Command::new(&app.config.editor)
                    .arg(linearg)
                    .arg(file_path)
                    .status()
                    .expect("Failed to start selected editor");

                app.file_str_list_state.select(None);
                // Clear terminal on exit from editor
                let _ = terminal.clear();
                let _ = terminal.draw(|frame| dispatch_render(app, frame));
            } else {
                app.start_async_search();
            }
        }
        KeyEvent {
            code: KeyCode::Esc, ..
        } => {
            // Exit when Esc is pressed
            app.exit = true;
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
            match app.switch_focus {
                Focus::Recentfiles => {
                    if app.recent_files.is_empty() {
                        return;
                    }
                }
                Focus::Filelist => {
                    if app.file_list.is_empty() {
                        return;
                    }
                }
                Focus::Filestrlist => {
                    if app.file_str_list.is_empty() {
                        return;
                    }
                }
            }

            app.hltd_file += 1;

            match app.switch_focus {
                Focus::Recentfiles => {
                    if app.hltd_file >= app.recent_files.len() && !app.recent_files.is_empty() {
                        app.hltd_file = app.recent_files.len() - 1;
                    }
                    app.recent_state.select(Some(app.hltd_file));
                }
                Focus::Filelist => {
                    if app.hltd_file >= app.file_list.len() && !app.file_list.is_empty() {
                        app.hltd_file = app.file_list.len() - 1;
                    }
                    app.file_list_state.select(Some(app.hltd_file));
                }
                Focus::Filestrlist => {
                    if app.hltd_file >= app.file_str_list.len() && !app.file_str_list.is_empty() {
                        app.hltd_file = app.file_str_list.len() - 1;
                    }
                    app.file_str_list_state.select(Some(app.hltd_file));
                }
            }
            app.preview = app.run_preview_cmd();
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
            match app.switch_focus {
                Focus::Recentfiles => {
                    if app.recent_files.is_empty() {
                        return;
                    }
                }
                Focus::Filelist => {
                    if app.file_list.is_empty() {
                        return;
                    }
                }
                Focus::Filestrlist => {
                    if app.file_str_list.is_empty() {
                        return;
                    }
                }
            }

            if app.hltd_file == 0 {
                return;
            }

            app.hltd_file -= 1;
            match app.switch_focus {
                Focus::Recentfiles => {
                    app.recent_state.select(Some(app.hltd_file));
                }
                Focus::Filelist => {
                    app.file_list_state.select(Some(app.hltd_file));
                }
                Focus::Filestrlist => {
                    app.file_str_list_state.select(Some(app.hltd_file));
                }
            }
            app.preview = app.run_preview_cmd();
        }
        KeyEvent {
            code: KeyCode::Tab, ..
        } => {
            // Switch between recent and search files
            match app.switch_focus {
                Focus::Recentfiles => {
                    if !app.file_str_list.is_empty() {
                        app.switch_focus = Focus::Filestrlist;
                    }
                    if !app.file_list.is_empty() {
                        app.switch_focus = Focus::Filelist;
                    }
                }
                Focus::Filelist => {
                    if !app.recent_files.is_empty() {
                        app.switch_focus = Focus::Recentfiles;
                    }

                    if !app.file_str_list.is_empty() {
                        app.switch_focus = Focus::Filestrlist;
                    }
                }
                Focus::Filestrlist => {
                    if !app.file_list.is_empty() {
                        app.switch_focus = Focus::Filelist;
                    }
                    if !app.recent_files.is_empty() {
                        app.switch_focus = Focus::Recentfiles;
                    }
                }
            }

            match app.switch_focus {
                Focus::Recentfiles => {
                    app.file_list_state.select(None);
                    app.file_str_list_state.select(None);
                    app.hltd_file = 0;
                    app.recent_state.select(Some(app.hltd_file));
                    if app.recent_files.is_empty() {
                        return;
                    }
                }
                Focus::Filelist => {
                    app.file_str_list_state.select(None);
                    app.recent_state.select(None);
                    app.hltd_file = 0;
                    app.file_list_state.select(Some(app.hltd_file));
                    if app.file_list.is_empty() {
                        return;
                    }
                }
                Focus::Filestrlist => {
                    app.file_list_state.select(None);
                    app.recent_state.select(None);
                    app.hltd_file = 0;
                    app.file_str_list_state.select(Some(app.hltd_file));
                    if app.file_str_list.is_empty() {
                        return;
                    }
                }
            }
            app.preview = app.run_preview_cmd();
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
            code: KeyCode::Char('f'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => {
            app.typed_input.clear();
            app.file_str_list.clear();
            app.search_progress_str.clear();
            app.prev_context = app.switch_context;
            app.switch_context = Context::Fileviewer;
            app.file_list = app.run_search_cmd();
        }
        KeyEvent {
            code: KeyCode::Char('x'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => {
            if app.switch_focus == Focus::Recentfiles {
                if app.recent_files.len() > 0 {
                    app.recent_files.remove(app.hltd_file);
                    app.hltd_file = 0;
                    app.recent_state.select(Some(app.hltd_file));
                }
            }
        }
        KeyEvent {
            code: KeyCode::Char('h'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => {
            if app.switch_context == Context::Help {
                app.prev_context = Context::Help;
                app.switch_context = app.prev_context;
            } else {
                app.prev_context = app.switch_context;
                app.switch_context = Context::Help;
            }
        }
        _ => {}
    };
}
