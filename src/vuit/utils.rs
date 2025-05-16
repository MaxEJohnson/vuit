use ratatui::style::Color;
use regex::Regex;
use std::path::PathBuf;

// Helper Functions
pub fn clean_utf8_content(content: &str) -> String {
    content
        .chars()
        .filter(|&c| c.is_ascii_graphic() || c == '\n' || c == ' ')
        .collect()
}
pub fn remove_ansi_escape_codes(input: &str) -> String {
    // Create a regex to match ANSI escape sequences
    let re = Regex::new(r"\x1b\[([0-9]{1,2};[0-9]{1,2}|[0-9]{1,2})?m").unwrap();
    let reclean = re.replace_all(input, "");
    let reclean = reclean.replace("\r", ""); // Remove carriage returns
    let reclean = reclean.replace("\t", "    "); // Convert tabs to spaces

    // Return the cleaned output
    reclean.to_string()
}
pub fn expand_tilde(path: &str) -> PathBuf {
    if path.starts_with("~") {
        if let Some(home_dir) = dirs::home_dir() {
            return home_dir.join(&path[2..]); // Replace `~` with the home directory
        }
    }
    PathBuf::from(path)
}

pub fn grab_config_color(color_str: &str) -> Color {
    match color_str.to_lowercase().as_str() {
        "lightblue" => Color::LightBlue,
        "blue" => Color::Blue,
        "lightred" => Color::LightRed,
        "red" => Color::Green,
        "lightgreen" => Color::LightGreen,
        "green" => Color::Green,
        "lightcyan" => Color::LightCyan,
        "cyan" => Color::Cyan,
        "lightyellow" => Color::LightYellow,
        "yellow" => Color::Yellow,
        "gray" => Color::Gray,
        "white" => Color::White,
        &_ => Color::LightBlue,
    }
}
