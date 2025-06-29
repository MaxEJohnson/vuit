use crate::vuit::contexts::terminal::send_cmd_to_proc_term;
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
use std::env;
use std::process::Command;

pub fn render(app: &mut Vuit, frame: &mut Frame, chunks: &[Rect]) {
    let area_height = chunks[1].height as usize;
    let area_width = chunks[1].width as usize;
    let total = app.file_list.len();
    let selected = if Focus::Filelist == app.switch_focus {
        app.hltd_file.min(total.saturating_sub(1))
    } else {
        0
    };

    if Focus::Filelist == app.switch_focus {}
    let start = if selected >= area_height {
        selected + 1 - area_height
    } else {
        0
    };

    let end = (start + area_height).min(total);
    let visible = &app.file_list[start..end];

    let truncated: Vec<String> = visible
        .iter()
        .map(|line| {
            if line.len() > (area_width - 5) {
                format!("…{}", &line[line.len() - (area_width - 5)..])
            } else {
                line.clone()
            }
        })
        .collect();

    if app.switch_focus == Focus::Filelist {
        app.file_list_state.select(Some(selected - start));
    }

    let block = Block::bordered()
        .title(Line::from(" Files ").centered())
        .border_set(border::ROUNDED);

    let list = List::new(truncated)
        .block(block)
        .style(Style::default().fg(grab_config_color(&app.config.colorscheme)))
        .highlight_style(
            Style::default()
                .fg(Color::White)
                .bg(grab_config_color(&app.config.highlight_color)),
        );

    frame.render_stateful_widget(list, chunks[1], &mut app.file_list_state);
}

pub fn handler(app: &mut Vuit, key: KeyEvent, terminal: &mut DefaultTerminal) {
    match key {
        KeyEvent {
            code: KeyCode::Char(c),
            modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT,
            ..
        } => {
            // FZF search after each keyswipe

            app.typed_input.push(c);
            app.file_list = app.run_search_cmd();

            match app.switch_focus {
                Focus::Recentfiles => {
                    app.recent_state.select(Some(app.hltd_file));
                    if app.hltd_file >= app.recent_files.len() && !app.recent_files.is_empty() {
                        app.hltd_file = app.recent_files.len() - 1;
                    }
                }
                Focus::Filelist => {
                    app.file_list_state.select(Some(app.hltd_file));
                    if app.hltd_file >= app.file_list.len() && !app.file_list.is_empty() {
                        app.hltd_file = app.file_list.len() - 1;
                    }
                }
                Focus::Filestrlist => {
                    app.file_str_list_state.select(Some(app.hltd_file));
                    if app.hltd_file >= app.file_str_list.len() && !app.file_str_list.is_empty() {
                        app.hltd_file = app.file_str_list.len() - 1;
                    }
                }
            }
            app.preview = app.run_preview_cmd();
        }
        KeyEvent {
            code: KeyCode::Backspace,
            ..
        } => {
            // FZF search after each Backspace

            if app.typed_input.is_empty() {
                return;
            }

            app.typed_input.pop();
            app.file_list = app.run_search_cmd();

            match app.switch_focus {
                Focus::Recentfiles => {
                    app.recent_state.select(Some(app.hltd_file));
                    if app.hltd_file >= app.recent_files.len() && !app.recent_files.is_empty() {
                        app.hltd_file = app.recent_files.len() - 1;
                    }
                }
                Focus::Filelist => {
                    app.file_list_state.select(Some(app.hltd_file));
                    if app.hltd_file >= app.file_list.len() && !app.file_list.is_empty() {
                        app.hltd_file = app.file_list.len() - 1;
                    }
                }
                Focus::Filestrlist => {
                    app.file_str_list_state.select(Some(app.hltd_file));
                    if app.hltd_file >= app.file_str_list.len() && !app.file_str_list.is_empty() {
                        app.hltd_file = app.file_str_list.len() - 1;
                    }
                }
            }
            app.preview = app.run_preview_cmd();
        }
        KeyEvent {
            code: KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            ..
        } => {
            match app.switch_focus {
                Focus::Recentfiles => {
                    if app.hltd_file >= app.recent_files.len() {
                        return;
                    }
                    if std::env::var("TMUX").is_ok() {
                        let tmux_cmd = format!(
                            "tmux split-window -h '{}' '{}' \\; resize-pane -t ! -x $(( $(tput cols) * 20/100 ))",
                            &app.config.editor,
                            &app.recent_files[app.hltd_file]
                            );
                        let _ = Command::new("sh")
                            .args(["-c", &tmux_cmd])
                            .status()
                            .expect("Failed to start selected editor");
                    } else {
                        let _ = Command::new(&app.config.editor)
                            .arg(&app.recent_files[app.hltd_file])
                            .status()
                            .expect("Failed to start selected editor");
                    }
                }
                Focus::Filelist => {
                    if app.hltd_file >= app.file_list.len() {
                        return;
                    }
                    if std::env::var("TMUX").is_ok() {
                        let tmux_cmd = format!(
                            "tmux split-window -h '{}' '{}' \\; resize-pane -t ! -x $(( $(tput cols) * 20/100 ))",
                            &app.config.editor,
                            &app.file_list[app.hltd_file]
                            );
                        let _ = Command::new("sh")
                            .args(["-c", &tmux_cmd])
                            .status()
                            .expect("Failed to start selected editor");
                    } else {
                        let _ = Command::new(&app.config.editor)
                            .arg(&app.file_list[app.hltd_file])
                            .status()
                            .expect("Failed to start selected editor");
                    }
                    let _ = terminal.clear();
                    let _ = terminal.draw(|frame| dispatch_render(app, frame));
                }
                Focus::Filestrlist => {
                    if app.hltd_file >= app.file_str_list.len() {
                        return;
                    }
                    let file_path = &app.file_str_list[app.hltd_file]
                        .split_once(':')
                        .map(|(before, _)| before)
                        .unwrap_or(app.file_str_list[app.hltd_file].as_str());
                    if std::env::var("TMUX").is_ok() {
                        let tmux_cmd = format!(
                            "tmux split-window -h '{}' '{}' \\; resize-pane -t ! -x $(( $(tput cols) * 20/100 ))",
                            &app.config.editor,
                            file_path,
                            );
                        let _ = Command::new("sh")
                            .args(["-c", &tmux_cmd])
                            .status()
                            .expect("Failed to start selected editor");
                    } else {
                        let _ = Command::new(&app.config.editor)
                            .arg(file_path)
                            .status()
                            .expect("Failed to start selected editor");
                    }
                }
            }

            if app.switch_focus == Focus::Filelist
                && !app.recent_files.contains(&app.file_list[app.hltd_file])
            {
                app.recent_files
                    .push(app.file_list[app.hltd_file].to_owned());
            }

            if app.recent_files.len() > 5 {
                app.recent_files.remove(0);
            }

            // Clear terminal on exit from editor
            let _ = terminal.clear();
            let _ = terminal.draw(|frame| dispatch_render(app, frame));
        }
        KeyEvent {
            code: KeyCode::Char('f'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => {
            app.current_filter = app.typed_input.clone();
            app.typed_input.clear();
            app.prev_context = app.switch_context;
            app.switch_context = Context::Stringsearch;
        }
        KeyEvent {
            code: KeyCode::Char('p'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => {
            app.preview_toggle = !app.preview_toggle;
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
            code: KeyCode::Char('t'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => {
            if std::env::var("TMUX").is_ok() {
                let _ = Command::new("tmux")
                    .args(["split-window", "-h"])
                    .status()
                    .expect("Failed to start terminal");
            } else {
                app.typed_input.clear();
                app.prev_context = app.switch_context;
                app.switch_context = Context::Terminal;
                app.term_out.clear();
            }
        }
        KeyEvent {
            code: KeyCode::Char('x'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => {
            let cwd = env::current_dir().expect("Failed to get current directory");
            let abs_path = match app.switch_focus {
                Focus::Recentfiles => cwd.join(app.recent_files[app.hltd_file].clone()),
                Focus::Filelist => cwd.join(app.file_list[app.hltd_file].clone()),
                Focus::Filestrlist => cwd.join(app.file_str_list[app.hltd_file].clone()),
            };
            let abs_path = abs_path
                .to_str()
                .expect("Path is not valid UTF-8")
                .to_string();

            if std::env::var("TMUX").is_ok() {
                let _ = Command::new("tmux")
                    .args(["split-window", "-h", "bash", "-c", &abs_path])
                    .status()
                    .expect("Failed to start terminal");
            } else {
                app.typed_input.clear();
                app.prev_context = app.switch_context;
                app.switch_context = Context::Terminal;
                app.typed_input = abs_path.clone();
            }
            send_cmd_to_proc_term(app);
            app.typed_input.clear();
            app.first_term_open = false;
        }
        KeyEvent {
            code: KeyCode::Char('h'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => {
            if app.switch_context == Context::Help {
                app.switch_context = app.prev_context;
            } else {
                app.prev_context = app.switch_context;
                app.switch_context = Context::Help;
            }
        }
        _ => {}
    };
}
