use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
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
    let mut pending: Option<PendingInput> = None;

    loop {
        let prompt = match &pending {
            Some(p) => p.prompt(),
            None => state.prompt(),
        };

        if let Some(helper) = editor.helper_mut() {
            helper.update_language(state.current_language().canonical_id().to_string());
        }

        match editor.readline(&prompt) {
            Ok(line) => {
                let raw = line.trim_end_matches(['\r', '\n']);

                if let Some(p) = pending.as_mut() {
                    if raw.trim() == ":cancel" {
                        pending = None;
                        continue;
                    }

                    p.push_line_auto(state.current_language().canonical_id(), raw);
                    if p.needs_more_input(state.current_language().canonical_id()) {
                        continue;
                    }

                    let code = p.take();
                    pending = None;
                    let trimmed = code.trim_end();
                    if !trimmed.is_empty() {
                        let _ = editor.add_history_entry(trimmed);
                        state.execute_snippet(trimmed)?;
                    }
                    continue;
                }

                if raw.trim().is_empty() {
                    continue;
                }

                if raw.trim_start().starts_with(':') {
                    let trimmed = raw.trim();
                    let _ = editor.add_history_entry(trimmed);
                    if state.handle_meta(trimmed)? {
                        break;
                    }
                    continue;
                }

                let mut p = PendingInput::new();
                p.push_line(raw);
                if p.needs_more_input(state.current_language().canonical_id()) {
                    pending = Some(p);
                    continue;
                }

                let trimmed = raw.trim_end();
                let _ = editor.add_history_entry(trimmed);
                state.execute_snippet(trimmed)?;
            }
            Err(ReadlineError::Interrupted) => {
                println!("^C");
                pending = None;
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

struct PendingInput {
    buf: String,
}

impl PendingInput {
    fn new() -> Self {
        Self { buf: String::new() }
    }

    fn prompt(&self) -> String {
        "... ".to_string()
    }

    fn push_line(&mut self, line: &str) {
        self.buf.push_str(line);
        self.buf.push('\n');
    }

    fn push_line_auto(&mut self, language_id: &str, line: &str) {
        match language_id {
            "python" | "py" | "python3" | "py3" => {
                let adjusted = python_auto_indent(line, &self.buf);
                self.push_line(&adjusted);
            }
            _ => self.push_line(line),
        }
    }

    fn take(&mut self) -> String {
        std::mem::take(&mut self.buf)
    }

    fn needs_more_input(&self, language_id: &str) -> bool {
        needs_more_input(language_id, &self.buf)
    }
}

fn needs_more_input(language_id: &str, code: &str) -> bool {
    match language_id {
        "python" | "py" | "python3" | "py3" => needs_more_input_python(code),

        _ => has_unclosed_delimiters(code) || generic_line_looks_incomplete(code),
    }
}

fn generic_line_looks_incomplete(code: &str) -> bool {
    let mut last: Option<&str> = None;
    for line in code.lines().rev() {
        let trimmed = line.trim_end();
        if trimmed.trim().is_empty() {
            continue;
        }
        last = Some(trimmed);
        break;
    }
    let Some(line) = last else { return false };
    let line = line.trim();
    if line.is_empty() {
        return false;
    }

    if line.ends_with('\\') {
        return true;
    }

    const TAILS: [&str; 24] = [
        "=", "+", "-", "*", "/", "%", "&", "|", "^", "!", "<", ">", "&&", "||", "??", "?:", "?",
        ":", ".", ",", "=>", "->", "::", "..",
    ];
    if TAILS.iter().any(|tok| line.ends_with(tok)) {
        return true;
    }

    const PREFIXES: [&str; 9] = [
        "return", "throw", "yield", "await", "import", "from", "export", "case", "else",
    ];
    let lowered = line.to_ascii_lowercase();
    if PREFIXES
        .iter()
        .any(|kw| lowered == *kw || lowered.ends_with(&format!(" {kw}")))
    {
        return true;
    }

    false
}

fn needs_more_input_python(code: &str) -> bool {
    if has_unclosed_delimiters(code) {
        return true;
    }

    let mut last_nonempty: Option<&str> = None;
    let mut saw_colon_header = false;

    for line in code.lines() {
        let trimmed = line.trim_end();
        if trimmed.trim().is_empty() {
            continue;
        }
        last_nonempty = Some(trimmed);
        if trimmed.ends_with(':') {
            saw_colon_header = true;
        }
    }

    if !saw_colon_header {
        return false;
    }

    if code.ends_with("\n\n") {
        return false;
    }

    last_nonempty.is_some()
}

fn python_auto_indent(line: &str, existing: &str) -> String {
    let trimmed = line.trim_end_matches(['\r', '\n']);
    let raw = trimmed;
    if raw.trim().is_empty() {
        return raw.to_string();
    }

    if raw.starts_with(' ') || raw.starts_with('\t') {
        return raw.to_string();
    }

    let mut last_nonempty: Option<&str> = None;
    for l in existing.lines().rev() {
        if l.trim().is_empty() {
            continue;
        }
        last_nonempty = Some(l);
        break;
    }

    let Some(prev) = last_nonempty else {
        return raw.to_string();
    };
    let prev_trimmed = prev.trim_end();

    if !prev_trimmed.ends_with(':') {
        return raw.to_string();
    }

    let lowered = raw.trim().to_ascii_lowercase();
    if lowered.starts_with("else:")
        || lowered.starts_with("elif ")
        || lowered.starts_with("except")
        || lowered.starts_with("finally:")
    {
        return raw.to_string();
    }

    let base_indent = prev
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect::<String>();

    format!("{base_indent}    {raw}")
}

fn has_unclosed_delimiters(code: &str) -> bool {
    let mut paren = 0i32;
    let mut bracket = 0i32;
    let mut brace = 0i32;

    let mut in_single = false;
    let mut in_double = false;
    let mut escape = false;

    for ch in code.chars() {
        if escape {
            escape = false;
            continue;
        }

        if in_single {
            if ch == '\\' {
                escape = true;
            } else if ch == '\'' {
                in_single = false;
            }
            continue;
        }
        if in_double {
            if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_double = false;
            }
            continue;
        }

        match ch {
            '\'' => in_single = true,
            '"' => in_double = true,
            '(' => paren += 1,
            ')' => paren -= 1,
            '[' => bracket += 1,
            ']' => bracket -= 1,
            '{' => brace += 1,
            '}' => brace -= 1,
            _ => {}
        }
    }

    paren > 0 || bracket > 0 || brace > 0
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
                            "[auto-detect] switching {} -> {}",
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

    #[test]
    fn python_multiline_def_requires_blank_line_to_execute() {
        let mut p = PendingInput::new();
        p.push_line("def fib(n):");
        assert!(p.needs_more_input("python"));
        p.push_line("    return n");
        assert!(p.needs_more_input("python"));
        p.push_line(""); // blank line ends block
        assert!(!p.needs_more_input("python"));
    }

    #[test]
    fn python_auto_indents_first_line_after_colon_header() {
        let mut p = PendingInput::new();
        p.push_line("def cool():");
        p.push_line_auto("python", r#"print("ok")"#);
        let code = p.take();
        assert!(
            code.contains("    print(\"ok\")\n"),
            "expected auto-indented print line, got:\n{code}"
        );
    }

    #[test]
    fn generic_multiline_tracks_unclosed_delimiters() {
        let mut p = PendingInput::new();
        p.push_line("func(");
        assert!(p.needs_more_input("csharp"));
        p.push_line(")");
        assert!(!p.needs_more_input("csharp"));
    }

    #[test]
    fn generic_multiline_tracks_trailing_equals() {
        let mut p = PendingInput::new();
        p.push_line("let x =");
        assert!(p.needs_more_input("rust"));
        p.push_line("10;");
        assert!(!p.needs_more_input("rust"));
    }

    #[test]
    fn generic_multiline_tracks_trailing_dot() {
        let mut p = PendingInput::new();
        p.push_line("foo.");
        assert!(p.needs_more_input("csharp"));
        p.push_line("Bar()");
        assert!(!p.needs_more_input("csharp"));
    }
}
