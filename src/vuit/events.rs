use crate::vuit::{Vuit, CONTEXT};
use crossterm::event::{self, Event, KeyEventKind};
use ratatui::DefaultTerminal;

use crate::vuit::contexts::{fileviewer, stringsearch, terminal};

pub fn dispatch_event(app: &mut Vuit, terminal: &mut DefaultTerminal) -> std::io::Result<()> {
    if !event::poll(std::time::Duration::from_millis(100))? {
        return Ok(());
    }

    if let Event::Key(key_event) = event::read()? {
        if key_event.kind != KeyEventKind::Press {
            return Ok(());
        }

        match app.switch_context {
            CONTEXT::FILEVIEWER => {
                fileviewer::handler(app, key_event, terminal);
            }
            CONTEXT::STRINGSEARCH => {
                stringsearch::handler(app, key_event, terminal);
            }
            CONTEXT::TERMINAL => {
                terminal::handler(app, key_event, terminal);
            }
            CONTEXT::HELP => {
                fileviewer::handler(app, key_event, terminal);
            }
        }
    }

    Ok(())
}
