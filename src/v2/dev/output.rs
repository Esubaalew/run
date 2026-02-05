//! Output Manager
//!
//! Colored terminal output for dev mode.

use std::time::Duration;

#[derive(Debug, Clone, Copy)]
pub enum OutputStyle {
    System,
    Component,
    Error,
    Warning,
    Success,
    Call,
}

#[derive(Debug, Clone)]
pub struct OutputLine {
    pub timestamp: u64,

    pub source: String,

    pub style: OutputStyle,

    pub message: String,
}

#[derive(Clone)]
pub struct OutputManager {
    verbose: bool,

    color: bool,
}

impl OutputManager {
    pub fn new(verbose: bool) -> Self {
        let color = std::env::var("NO_COLOR").is_err() && atty_is_terminal();

        Self { verbose, color }
    }

    pub fn is_verbose(&self) -> bool {
        self.verbose
    }

    pub fn print_banner(&self, project_name: &str, startup_time: Duration) {
        println!();
        if self.color {
            println!("\x1b[1;36m+------------------------------------------+\x1b[0m");
            println!(
                "\x1b[1;36m|\x1b[0m  \x1b[1;37mRun 2.0\x1b[0m - WASI Universal Runtime        \x1b[1;36m|\x1b[0m"
            );
            println!(
                "\x1b[1;36m|\x1b[0m  \x1b[90mProject:\x1b[0m \x1b[1m{:<27}\x1b[0m \x1b[1;36m|\x1b[0m",
                project_name
            );
            println!(
                "\x1b[1;36m|\x1b[0m  \x1b[90mStartup:\x1b[0m \x1b[32m{:?}\x1b[0m                       \x1b[1;36m|\x1b[0m",
                startup_time
            );
            println!("\x1b[1;36m+------------------------------------------+\x1b[0m");
        } else {
            println!("+------------------------------------------+");
            println!("|  Run 2.0 - WASI Universal Runtime        |");
            println!("|  Project: {:<27} |", project_name);
            println!("|  Startup: {:?}                       |", startup_time);
            println!("+------------------------------------------+");
        }
        println!();
    }

    pub fn print_ready(&self) {
        println!();
        if self.color {
            println!("\x1b[1;32mDevelopment server ready\x1b[0m");
            println!("\x1b[90m  Press Ctrl+C to stop\x1b[0m");
        } else {
            println!("Development server ready");
            println!("  Press Ctrl+C to stop");
        }
        println!();
    }

    pub fn log_system(&self, message: &str) {
        if self.color {
            println!("\x1b[90m[dev]\x1b[0m {}", message);
        } else {
            println!("[dev] {}", message);
        }
    }

    pub fn log_component(&self, component: &str, message: &str) {
        let color = component_color(component);

        if self.color {
            println!("\x1b[1;{}m[dev]\x1b[0m {} {}", color, component, message);
        } else {
            println!("[dev] {} {}", component, message);
        }
    }

    pub fn log_error(&self, source: &str, message: &str) {
        if self.color {
            println!(
                "\x1b[1;31m[dev]\x1b[0m {} \x1b[31mFAILED: {}\x1b[0m",
                source, message
            );
        } else {
            println!("[dev] {} FAILED: {}", source, message);
        }
    }

    pub fn log_warning(&self, source: &str, message: &str) {
        if self.color {
            println!(
                "\x1b[1;33m[dev]\x1b[0m {} \x1b[33m{}\x1b[0m",
                source, message
            );
        } else {
            println!("[dev] {} {}", source, message);
        }
    }

    pub fn log_call(&self, from: &str, to: &str, function: &str) {
        if !self.verbose {
            return;
        }

        let timestamp = format_timestamp();
        if self.color {
            println!(
                "\x1b[90m{}\x1b[0m \x1b[90m[call]\x1b[0m {} -> {}::{}",
                timestamp, from, to, function
            );
        } else {
            println!("{} [call] {} -> {}::{}", timestamp, from, to, function);
        }
    }

    pub fn log_stdout(&self, component: &str, data: &[u8]) {
        let color = component_color(component);
        let text = String::from_utf8_lossy(data);

        for line in text.lines() {
            if self.color {
                println!("\x1b[{}m|\x1b[0m {}", color, line);
            } else {
                println!("| {}", line);
            }
        }
    }

    pub fn log_stderr(&self, _component: &str, data: &[u8]) {
        let text = String::from_utf8_lossy(data);

        for line in text.lines() {
            if self.color {
                println!("\x1b[31m|\x1b[0m {}", line);
            } else {
                println!("| {}", line);
            }
        }
    }
}

fn format_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();

    let secs = now.as_secs();
    let hours = (secs / 3600) % 24;
    let mins = (secs / 60) % 60;
    let secs = secs % 60;
    let ms = now.subsec_millis();

    format!("{:02}:{:02}:{:02}.{:03}", hours, mins, secs, ms)
}

fn component_color(name: &str) -> u8 {
    let hash: u32 = name.bytes().fold(0u32, |acc, b| acc.wrapping_add(b as u32));
    let colors = [32, 33, 34, 35, 36, 37]; // green, yellow, blue, magenta, cyan, white
    colors[(hash as usize) % colors.len()]
}

fn atty_is_terminal() -> bool {
    std::env::var("TERM").is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_component_color() {
        let c1 = component_color("api");
        let c2 = component_color("api");
        assert_eq!(c1, c2);

        let c3 = component_color("worker");
        assert_eq!(c3, component_color("worker"));
    }

    #[test]
    fn test_format_timestamp() {
        let ts = format_timestamp();
        assert_eq!(ts.len(), 12);
        assert_eq!(&ts[2..3], ":");
        assert_eq!(&ts[5..6], ":");
        assert_eq!(&ts[8..9], ".");
    }
}
