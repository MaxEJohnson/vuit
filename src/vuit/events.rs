use crate::vuit::{Context, Vuit};
use crossterm::event::{self, Event, KeyEventKind};
use ratatui::DefaultTerminal;
use std::sync::atomic::Ordering;

use crate::vuit::contexts::{fileviewer, stringsearch, stringsearchreplace, terminal};

pub fn dispatch_event(app: &mut Vuit, terminal: &mut DefaultTerminal) -> std::io::Result<()> {
    if !event::poll(std::time::Duration::from_millis(100))? {
        if app.search_in_progress
            && app.search_progress.load(Ordering::Relaxed) == app.file_list.len()
        {
            if let Ok(mut result) = app.search_result.lock() {
                if let Some(data) = result.take() {
                    app.file_str_list = data;
                    app.search_in_progress = false;
                }
            }
        }
        return Ok(());
    }

    if let Event::Key(key_event) = event::read()? {
        if key_event.kind != KeyEventKind::Press {
            return Ok(());
        }

        match app.switch_context {
            Context::Fileviewer => {
                fileviewer::handler(app, key_event, terminal);
            }
            Context::Stringsearch => {
                stringsearch::handler(app, key_event, terminal);
            }
            Context::Stringsearchreplace => {
                stringsearchreplace::handler(app, key_event, terminal);
            }
            Context::Terminal => {
                terminal::handler(app, key_event, terminal);
            }
            Context::Help => {
                fileviewer::handler(app, key_event, terminal);
            }
        }
    }

    Ok(())
}
