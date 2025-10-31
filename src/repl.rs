use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use rustyline::completion::Completer;
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{Editor, Helper};

use crate::engine::{ExecutionOutcome, ExecutionPayload, LanguageRegistry, LanguageSession};
use crate::highlight;
use crate::language::LanguageSpec;

const HISTORY_FILE: &str = ".run_history";

struct ReplHelper {
    language_id: String,
}

impl ReplHelper {
    fn new(language_id: String) -> Self {
        Self { language_id }
    }

    fn update_language(&mut self, language_id: String) {
        self.language_id = language_id;
    }
}

impl Completer for ReplHelper {
    type Candidate = String;
}

impl Hinter for ReplHelper {
    type Hint = String;
}

impl Validator for ReplHelper {}

impl Highlighter for ReplHelper {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        if line.trim_start().starts_with(':') {
            return Cow::Borrowed(line);
        }

        let highlighted = highlight::highlight_repl_input(line, &self.language_id);
        Cow::Owned(highlighted)
    }

    fn highlight_char(&self, _line: &str, _pos: usize, _forced: bool) -> bool {
        true
    }
}

impl Helper for ReplHelper {}

pub fn run_repl(
    initial_language: LanguageSpec,
    registry: LanguageRegistry,
    detect_enabled: bool,
) -> Result<i32> {
    let helper = ReplHelper::new(initial_language.canonical_id().to_string());
    let mut editor = Editor::<ReplHelper, DefaultHistory>::new()?;
    editor.set_helper(Some(helper));

    if let Some(path) = history_path() {
        let _ = editor.load_history(&path);
    }

    println!("run universal REPL. Type :help for commands.");

    let mut state = ReplState::new(initial_language, registry, detect_enabled)?;

    loop {
        let prompt = state.prompt();

        if let Some(helper) = editor.helper_mut() {
            helper.update_language(state.current_language().canonical_id().to_string());
        }

        match editor.readline(&prompt) {
            Ok(line) => {
                let trimmed = line.trim_end();
                if trimmed.is_empty() {
                    continue;
                }
                let _ = editor.add_history_entry(trimmed);
                if trimmed.starts_with(':') {
                    if state.handle_meta(trimmed)? {
                        break;
                    }
                } else {
                    state.execute_snippet(trimmed)?;
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("^C");
                continue;
            }
            Err(ReadlineError::Eof) => {
                println!("bye");
                break;
            }
            Err(err) => {
                bail!("readline error: {err}");
            }
        }
    }

    if let Some(path) = history_path() {
        let _ = editor.save_history(&path);
    }

    state.shutdown();
    Ok(0)
}

struct ReplState {
    registry: LanguageRegistry,
    sessions: HashMap<String, Box<dyn LanguageSession>>, // keyed by canonical id
    current_language: LanguageSpec,
    detect_enabled: bool,
}

impl ReplState {
    fn new(
        initial_language: LanguageSpec,
        registry: LanguageRegistry,
        detect_enabled: bool,
    ) -> Result<Self> {
        let mut state = Self {
            registry,
            sessions: HashMap::new(),
            current_language: initial_language,
            detect_enabled,
        };
        state.ensure_current_language()?;
        Ok(state)
    }

    fn current_language(&self) -> &LanguageSpec {
        &self.current_language
    }

    fn prompt(&self) -> String {
        format!("{}>>> ", self.current_language.canonical_id())
    }

    fn ensure_current_language(&mut self) -> Result<()> {
        if self.registry.resolve(&self.current_language).is_none() {
            bail!(
                "language '{}' is not available",
                self.current_language.canonical_id()
            );
        }
        Ok(())
    }

    fn handle_meta(&mut self, line: &str) -> Result<bool> {
        let command = line.trim_start_matches(':').trim();
        if command.is_empty() {
            return Ok(false);
        }

        let mut parts = command.split_whitespace();
        let head = parts.next().unwrap();
        match head {
            "exit" | "quit" => return Ok(true),
            "help" => {
                self.print_help();
                return Ok(false);
            }
            "languages" => {
                self.print_languages();
                return Ok(false);
            }
            "detect" => {
                if let Some(arg) = parts.next() {
                    match arg {
                        "on" | "true" | "1" => {
                            self.detect_enabled = true;
                            println!("auto-detect enabled");
                        }
                        "off" | "false" | "0" => {
                            self.detect_enabled = false;
                            println!("auto-detect disabled");
                        }
                        "toggle" => {
                            self.detect_enabled = !self.detect_enabled;
                            println!(
                                "auto-detect {}",
                                if self.detect_enabled {
                                    "enabled"
                                } else {
                                    "disabled"
                                }
                            );
                        }
                        _ => println!("usage: :detect <on|off|toggle>"),
                    }
                } else {
                    println!(
                        "auto-detect is {}",
                        if self.detect_enabled {
                            "enabled"
                        } else {
                            "disabled"
                        }
                    );
                }
                return Ok(false);
            }
            "lang" => {
                if let Some(lang) = parts.next() {
                    self.switch_language(LanguageSpec::new(lang.to_string()))?;
                } else {
                    println!("usage: :lang <language>");
                }
                return Ok(false);
            }
            "reset" => {
                self.reset_current_session();
                println!(
                    "session for '{}' reset",
                    self.current_language.canonical_id()
                );
                return Ok(false);
            }
            "load" | "run" => {
                if let Some(token) = parts.next() {
                    let path = PathBuf::from(token);
                    self.execute_payload(ExecutionPayload::File { path })?;
                } else {
                    println!("usage: :load <path>");
                }
                return Ok(false);
            }
            alias => {
                // allow :py style switching for any registered alias
                let spec = LanguageSpec::new(alias);
                if self.registry.resolve(&spec).is_some() {
                    self.switch_language(spec)?;
                    return Ok(false);
                }
                println!("unknown command: :{alias}. Type :help for help.");
            }
        }

        Ok(false)
    }

    fn switch_language(&mut self, spec: LanguageSpec) -> Result<()> {
        if self.current_language.canonical_id() == spec.canonical_id() {
            println!("already using {}", spec.canonical_id());
            return Ok(());
        }
        if self.registry.resolve(&spec).is_none() {
            let available = self.registry.known_languages().join(", ");
            bail!(
                "language '{}' not supported. Available: {available}",
                spec.canonical_id()
            );
        }
        self.current_language = spec;
        println!("switched to {}", self.current_language.canonical_id());
        Ok(())
    }

    fn reset_current_session(&mut self) {
        let key = self.current_language.canonical_id().to_string();
        if let Some(mut session) = self.sessions.remove(&key) {
            let _ = session.shutdown();
        }
    }

    fn execute_snippet(&mut self, code: &str) -> Result<()> {
        if self.detect_enabled {
            if let Some(detected) = crate::detect::detect_language_from_snippet(code) {
                if detected != self.current_language.canonical_id() {
                    let spec = LanguageSpec::new(detected.to_string());
                    if self.registry.resolve(&spec).is_some() {
                        println!(
                            "[auto-detect] switching {} → {}",
                            self.current_language.canonical_id(),
                            spec.canonical_id()
                        );
                        self.current_language = spec;
                    }
                }
            }
        }
        let payload = ExecutionPayload::Inline {
            code: code.to_string(),
        };
        self.execute_payload(payload)
    }

    fn execute_payload(&mut self, payload: ExecutionPayload) -> Result<()> {
        let language = self.current_language.clone();
        let outcome = match payload {
            ExecutionPayload::Inline { code } => {
                if self.engine_supports_sessions(&language)? {
                    self.eval_in_session(&language, &code)?
                } else {
                    let engine = self
                        .registry
                        .resolve(&language)
                        .context("language engine not found")?;
                    engine.execute(&ExecutionPayload::Inline { code })?
                }
            }
            ExecutionPayload::File { path } => {
                let engine = self
                    .registry
                    .resolve(&language)
                    .context("language engine not found")?;
                engine.execute(&ExecutionPayload::File { path })?
            }
            ExecutionPayload::Stdin { code } => {
                let engine = self
                    .registry
                    .resolve(&language)
                    .context("language engine not found")?;
                engine.execute(&ExecutionPayload::Stdin { code })?
            }
        };
        render_outcome(&outcome);
        Ok(())
    }

    fn engine_supports_sessions(&self, language: &LanguageSpec) -> Result<bool> {
        Ok(self
            .registry
            .resolve(language)
            .context("language engine not found")?
            .supports_sessions())
    }

    fn eval_in_session(&mut self, language: &LanguageSpec, code: &str) -> Result<ExecutionOutcome> {
        use std::collections::hash_map::Entry;
        let key = language.canonical_id().to_string();
        match self.sessions.entry(key) {
            Entry::Occupied(mut entry) => entry.get_mut().eval(code),
            Entry::Vacant(entry) => {
                let engine = self
                    .registry
                    .resolve(language)
                    .context("language engine not found")?;
                let mut session = engine.start_session().with_context(|| {
                    format!("failed to start {} session", language.canonical_id())
                })?;
                let outcome = session.eval(code)?;
                entry.insert(session);
                Ok(outcome)
            }
        }
    }

    fn print_languages(&self) {
        let mut languages = self.registry.known_languages();
        languages.sort();
        println!("available languages: {}", languages.join(", "));
    }

    fn print_help(&self) {
        println!("Commands:");
        println!("  :help                 Show this help message");
        println!("  :languages            List available languages");
        println!("  :lang <id>            Switch to language <id>");
        println!("  :detect on|off        Enable or disable auto language detection");
        println!("  :reset                Reset the current language session");
        println!("  :load <path>          Execute a file in the current language");
        println!("  :exit, :quit          Leave the REPL");
        println!("Any language id or alias works as a shortcut, e.g. :py, :cpp, :csharp, :php.");
    }

    fn shutdown(&mut self) {
        for (_, mut session) in self.sessions.drain() {
            let _ = session.shutdown();
        }
    }
}

fn render_outcome(outcome: &ExecutionOutcome) {
    if !outcome.stdout.is_empty() {
        print!("{}", ensure_trailing_newline(&outcome.stdout));
    }
    if !outcome.stderr.is_empty() {
        eprint!("{}", ensure_trailing_newline(&outcome.stderr));
    }
    if let Some(code) = outcome.exit_code {
        if code != 0 {
            println!("[exit code {code}] ({}ms)", outcome.duration.as_millis());
        }
    }
}

fn ensure_trailing_newline(text: &str) -> String {
    if text.ends_with('\n') {
        text.to_string()
    } else {
        let mut owned = text.to_string();
        owned.push('\n');
        owned
    }
}

fn history_path() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("HOME") {
        return Some(Path::new(&home).join(HISTORY_FILE));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_aliases_resolve_in_registry() {
        let registry = LanguageRegistry::bootstrap();
        let aliases = [
            "python",
            "py",
            "python3",
            "rust",
            "rs",
            "go",
            "golang",
            "csharp",
            "cs",
            "c#",
            "typescript",
            "ts",
            "javascript",
            "js",
            "node",
            "ruby",
            "rb",
            "lua",
            "bash",
            "sh",
            "zsh",
            "java",
            "php",
            "kotlin",
            "kt",
            "c",
            "cpp",
            "c++",
            "swift",
            "swiftlang",
            "perl",
            "pl",
            "julia",
            "jl",
        ];

        for alias in aliases {
            let spec = LanguageSpec::new(alias);
            assert!(
                registry.resolve(&spec).is_some(),
                "alias {alias} should resolve to a registered language"
            );
        }
    }
}
