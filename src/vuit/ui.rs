use crate::vuit::{Context, Vuit};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    prelude::Rect,
    style::{Color, Modifier, Style},
    symbols::border,
    text::{Line, Text},
    widgets::{Block, List, Paragraph},
    DefaultTerminal, Frame,
};
use std::sync::atomic::Ordering;

use crate::vuit::contexts::{fileviewer, stringsearch, terminal};
use crate::vuit::utils::grab_config_color;
use crate::vuit::{
    HELP_TEXT_BOX_NUM_LINES, RECENT_BUFFERS_NUM_LINES, SEARCH_BAR_NUM_LINES, TERMINAL_NUM_LINES,
};

// Constants
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

pub fn dispatch_render(app: &mut Vuit, frame: &mut Frame) {
    let (chunks, _content_lines) = make_main_layout(app, frame);
    let top_chunks = make_top_chunks(&chunks);
    let left_chunks = make_left_chunks(&top_chunks);
    let search_terminal_chunks = make_search_terminal_chunks(app, &chunks);
    let search_split_help_chunks = make_search_split_help_chunks(&search_terminal_chunks);

    fileviewer::render(app, frame, &left_chunks);
    render_recent_files(app, frame, &left_chunks);
    render_preview_list(app, frame, &top_chunks);
    render_search_input(app, frame, &search_split_help_chunks);
    render_help_toggle_text_box(app, frame, &search_split_help_chunks);

    match app.switch_context {
        Context::Fileviewer => {
            render_file_count_display(app, frame, &left_chunks);
        }
        Context::Stringsearch => {
            stringsearch::render(app, frame, &search_terminal_chunks);
            render_search_progress_display(app, frame, &search_terminal_chunks);
        }
        Context::Terminal => {
            terminal::render(app, frame, &search_terminal_chunks);
        }
        Context::Help => {
            render_help_menu(app, frame, &search_terminal_chunks);
        }
    }
}

fn render_recent_files(app: &mut Vuit, f: &mut Frame, chunks: &[Rect]) {
    let block = Block::bordered()
        .title(Line::from(" Recent ").centered())
        .border_set(border::ROUNDED);
    let list = List::new(app.recent_files.to_owned())
        .block(block)
        .style(Style::default().fg(grab_config_color(&app.config.colorscheme)))
        .highlight_style(
            Style::default()
                .fg(Color::White)
                .bg(grab_config_color(&app.config.highlight_color)),
        );
    f.render_stateful_widget(list, chunks[0], &mut app.recent_state);
}

fn render_preview_list(app: &mut Vuit, f: &mut Frame, chunks: &[Rect]) {
    let block = Block::bordered()
        .title(Line::from(" Preview ").centered())
        .border_set(border::ROUNDED);
    let list = List::new(app.preview.to_owned())
        .block(block)
        .style(Style::default().fg(grab_config_color(&app.config.colorscheme)));
    f.render_widget(list, chunks[1]);
}

fn render_search_input(app: &mut Vuit, f: &mut Frame, chunks: &[Rect]) {
    let filter = if app.switch_context == Context::Stringsearch {
        let flt = if app.current_filter.is_empty() {
            "null".to_owned()
        } else {
            format!("\"{}\"", app.current_filter)
        };
        format!(" [FILE FILTER: {}] > {}", flt, app.typed_input)
    } else {
        format!(" > {}", app.typed_input)
    };

    let para = Paragraph::new(Text::from(filter))
        .block(
            Block::bordered()
                .title(Line::from(" Command Line ").left_aligned())
                .border_set(border::ROUNDED),
        )
        .style(Style::default().fg(grab_config_color(&app.config.colorscheme)));

    f.render_widget(para, chunks[0]);
}

fn render_help_toggle_text_box(app: &mut Vuit, f: &mut Frame, chunks: &[Rect]) {
    let box_widget = List::new(vec![" Help -> <C-h>"])
        .block(Block::bordered().border_set(border::ROUNDED))
        .style(
            Style::default()
                .fg(grab_config_color(&app.config.colorscheme))
                .add_modifier(Modifier::BOLD),
        );
    f.render_stateful_widget(box_widget, chunks[1], &mut app.help_menu_state);
}

fn render_help_menu(app: &mut Vuit, f: &mut Frame, chunks: &[Rect]) {
    app.help_menu = build_help_text();
    let list = List::new(app.help_menu.to_owned())
        .block(
            Block::bordered()
                .title(Line::from(" Help Menu ").centered())
                .border_set(border::ROUNDED),
        )
        .style(Style::default().fg(Color::White));
    f.render_widget(list, chunks[0]);
}

fn render_file_count_display(app: &mut Vuit, f: &mut Frame, chunks: &[Rect]) {
    let count = format!(" [ {} / {} ] ", app.file_list.len(), app.fd_list.len());
    let para = Paragraph::new(count)
        .block(Block::bordered().border_set(border::ROUNDED))
        .alignment(ratatui::prelude::Alignment::Center)
        .style(Style::default().fg(grab_config_color(&app.config.colorscheme)));

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

fn render_search_progress_display(app: &mut Vuit, f: &mut Frame, chunks: &[Rect]) {
    let status = if app.search_in_progress {
        let progress = app.search_progress.load(Ordering::Relaxed);
        format!(" [ {} / {} ] ", progress, app.file_list.len())
    } else {
        format!(" [ {} Matches ] ", app.file_str_list.len())
    };

    let para = Paragraph::new(status)
        .block(Block::bordered().border_set(border::ROUNDED))
        .alignment(ratatui::prelude::Alignment::Center)
        .style(Style::default().fg(grab_config_color(&app.config.colorscheme)));

    let filecount_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(chunks[0].height.saturating_sub(4)),
            Constraint::Length(3),
        ])
        .split(chunks[0]);

    let right_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(chunks[0].width.saturating_sub(30)),
            Constraint::Length(27),
        ])
        .split(filecount_chunks[1]);

    f.render_widget(para, right_chunks[1]);
}

fn build_help_text() -> Vec<String> {
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
        "<C-t> - Switches focus back to the file list, but terminal session is preserved".into(),
        "quit, exit - Switches focus back to the file list and restarts the terminal instance"
            .into(),
        "restart - If terminal seems unresponsive, this will restart the session".into(),
    ]
}

fn make_main_layout(app: &Vuit, frame: &Frame) -> (Vec<Rect>, u16) {
    let (search_lines, terminal_lines) = if app.switch_context == Context::Terminal
        || app.switch_context == Context::Help
        || app.switch_context == Context::Stringsearch
    {
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

fn make_top_chunks(chunks: &[Rect]) -> Vec<Rect> {
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[0])
        .to_vec()
}

fn make_left_chunks(top_chunks: &[Rect]) -> Vec<Rect> {
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

fn make_search_terminal_chunks(app: &Vuit, chunks: &[Rect]) -> Vec<Rect> {
    if app.switch_context == Context::Stringsearch
        || app.switch_context == Context::Terminal
        || app.switch_context == Context::Help
    {
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

fn make_search_split_help_chunks(search_terminal_chunks: &[Rect]) -> Vec<Rect> {
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

pub fn next_colorscheme(app: &mut Vuit, terminal: &mut DefaultTerminal) {
    app.colorscheme_index = (app.colorscheme_index + 1) % COLORS.len();
    app.config.colorscheme = COLORS[app.colorscheme_index].to_string();
    app.config.highlight_color = COLORS[(app.colorscheme_index + 1) % COLORS.len()].to_string();

    let _ = terminal.draw(|frame| dispatch_render(app, frame));
}
