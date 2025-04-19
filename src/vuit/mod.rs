// Modules
pub mod events;
pub mod ui;
pub mod utils;

pub mod contexts {
    pub mod fileviewer;
    pub mod stringsearch;
    pub mod stringsearchreplace;
    pub mod terminal;
}

// Vuit Imports
use crate::vuit::contexts::terminal::start_term;
use crate::vuit::events::dispatch_event;
use crate::vuit::ui::dispatch_render;
use crate::vuit::utils::{clean_utf8_content, expand_tilde};

// Std Lib
use std::{
    collections::HashMap,
    fs::{self, read_to_string, write, File},
    io::{self, BufRead, BufReader, Write},
    path::Path,
    sync::{Arc, Mutex},
    thread,
};

// Ratatui
use ratatui::{widgets::ListState, DefaultTerminal};

// External Crates
use clap::Command as ClapCommand;
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use ignore::{DirEntry, WalkBuilder};
use itertools::Itertools;
use memchr::memmem;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};

// Constants
const RECENT_BUFFERS_NUM_LINES: u16 = 8;
const TERMINAL_NUM_LINES: u16 = 20;
const SEARCH_BAR_NUM_LINES: u16 = 3;
const PREVIEW_NUM_LINES: u16 = 50;
const HELP_TEXT_BOX_NUM_LINES: u16 = 18;

// Focus States
#[derive(PartialEq, Eq, Default)]
enum Focus {
    Recentfiles,
    #[default]
    Filelist,
    Filestrlist,
}

// Context States
#[derive(PartialEq, Eq, Clone, Copy, Default)]
enum Context {
    #[default]
    Fileviewer,
    Stringsearch,
    Stringsearchreplace,
    Terminal,
    Help,
}

// Vuit Configuration
#[derive(Debug, Serialize, Deserialize)]
pub struct VuitRC {
    colorscheme: String,
    highlight_color: String,
    editor: String,
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

// Vuit Struct
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
    current_str_filter: String,
    search_progress_str: String,

    // Terminal vars
    bash_process: Option<Box<dyn portable_pty::Child + Send + Sync>>,
    process_out: Arc<Mutex<Vec<String>>>,
    command_sender: Arc<Mutex<Option<Box<dyn Write + Send>>>>,

    // String Search vars
    search_in_progress: bool,
    search_progress: Arc<AtomicUsize>,
    search_result: Arc<Mutex<Option<Vec<String>>>>,

    // State Variables
    switch_focus: Focus,
    switch_context: Context,
    prev_context: Context,
    hltd_file: usize,
    file_list_state: ListState,
    file_str_list_state: ListState,
    recent_state: ListState,
    help_menu_state: ListState,

    // Termination
    exit: bool,
}

// Implementing Vuit
impl Vuit {
    fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        // Initialize Focus to File List
        self.switch_focus = Focus::Filelist;

        // Initialize Context
        self.switch_context = Context::Fileviewer;

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
        start_term(self);

        // Start Vuit
        while !self.exit {
            terminal.draw(|frame| dispatch_render(self, frame))?;
            dispatch_event(self, terminal)?;
        }

        // Clear Terminal after close
        let _ = terminal.clear();

        Ok(())
    }

    fn skip_git(entry: &DirEntry) -> bool {
        if let Some(file_name) = entry.file_name().to_str() {
            file_name != ".git"
        } else {
            true
        }
    }

    fn run_fd_cmd(&mut self) {
        self.fd_list = WalkBuilder::new(".")
            .standard_filters(true)
            .hidden(false)
            .filter_entry(|entry| Vuit::skip_git(entry))
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

    fn start_async_search(&mut self) {
        let search = self.typed_input.to_lowercase();
        let file_list = self.file_list.clone();
        let progress = Arc::clone(&self.search_progress);
        let result = Arc::clone(&self.search_result);

        self.search_in_progress = true;

        progress.store(0, Ordering::Relaxed);
        thread::spawn(move || {
            let matches: Vec<String> = file_list
                .par_iter()
                .flat_map_iter(|path_str| {
                    let path = Path::new(path_str);
                    let file = match File::open(path) {
                        Ok(f) => f,
                        Err(_) => {
                            progress.fetch_add(1, Ordering::Relaxed);
                            return Some(vec![]);
                        }
                    };
                    let reader = BufReader::new(file);

                    let mut file_matches = Vec::new();

                    for (line_number, line) in reader.lines().enumerate() {
                        if let Ok(line) = line {
                            if memmem::find(line.to_lowercase().as_bytes(), search.as_bytes())
                                .is_some()
                            {
                                file_matches.push(clean_utf8_content(&format!(
                                    "{}:{}:{}",
                                    path.display(),
                                    line_number + 1,
                                    line
                                )));
                            }
                        }
                    }

                    progress.fetch_add(1, Ordering::Relaxed);
                    Some(file_matches)
                })
                .flatten()
                .collect();

            if let Ok(mut lock) = result.lock() {
                *lock = Some(matches);
            }
        });
    }

    fn replace_string_occurences(&mut self) {
        if self.current_str_filter.is_empty() {
            return;
        }

        let mut file_cache: HashMap<String, Vec<String>> = HashMap::new();

        for entry in self.file_str_list.iter() {
            let parts: Vec<&str> = entry.split(':').collect();
            if parts.len() < 3 {
                continue;
            }
            let file_path = parts[0].to_string();
            let line_number: usize = match parts[1].parse() {
                Ok(num) => num,
                Err(_) => continue,
            };

            let lines = file_cache.entry(file_path.clone()).or_insert_with(|| {
                read_to_string(&file_path)
                    .map(|content| content.lines().map(|line| line.to_string()).collect())
                    .unwrap_or_default()
            });

            if line_number == 0 || line_number > lines.len() {
                continue;
            }

            lines[line_number - 1] =
                lines[line_number - 1].replace(&self.current_str_filter, &self.typed_input);
        }

        for (filename, lines) in file_cache {
            let content = lines.join("\n");
            let _ = write(&filename, content);
        }
        self.file_str_list.clear();
        self.typed_input.clear();
    }

    fn run_preview_cmd(&mut self) -> Vec<String> {
        let file_list = match self.switch_focus {
            Focus::Recentfiles => &self.recent_files,
            Focus::Filelist => &self.file_list,
            Focus::Filestrlist => &self.file_str_list,
        };

        if file_list.is_empty() || self.switch_focus == Focus::Filestrlist {
            return vec![];
        }

        let file_path = &file_list[self.hltd_file];

        let num_lines =
            if self.switch_context == Context::Terminal || self.switch_context == Context::Help {
                PREVIEW_NUM_LINES - TERMINAL_NUM_LINES
            } else {
                PREVIEW_NUM_LINES
            };

        let num_lines: usize = num_lines as usize;

        match File::open(file_path) {
            Ok(file) => {
                if self.switch_focus == Focus::Filestrlist {
                    vec![]
                } else {
                    let reader = BufReader::new(file);
                    reader
                        .lines()
                        .take(num_lines)
                        .filter_map(Result::ok)
                        .map(|line| clean_utf8_content(&line))
                        .collect::<Vec<String>>()
                }
            }
            Err(_) => vec!["No Preview Available".to_string()],
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
