use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{Editor, Helper};

use crate::engine::{
    ExecutionOutcome, ExecutionPayload, LanguageRegistry, LanguageSession, build_install_command,
    package_install_command,
};
use crate::highlight;
use crate::language::LanguageSpec;

const HISTORY_FILE: &str = ".run_history";

struct ReplHelper {
    language_id: String,
    session_vars: Vec<String>,
}

impl ReplHelper {
    fn new(language_id: String) -> Self {
        Self {
            language_id,
            session_vars: Vec::new(),
        }
    }

    fn update_language(&mut self, language_id: String) {
        self.language_id = language_id;
    }

    fn update_session_vars(&mut self, vars: Vec<String>) {
        self.session_vars = vars;
    }
}

const META_COMMANDS: &[&str] = &[
    ":help",
    ":exit",
    ":quit",
    ":languages",
    ":lang ",
    ":detect ",
    ":reset",
    ":load ",
    ":run ",
    ":save ",
    ":history",
    ":install ",
    ":bench ",
    ":type",
];

fn language_keywords(lang: &str) -> &'static [&'static str] {
    match lang {
        "python" | "py" | "python3" | "py3" => &[
            "False",
            "None",
            "True",
            "and",
            "as",
            "assert",
            "async",
            "await",
            "break",
            "class",
            "continue",
            "def",
            "del",
            "elif",
            "else",
            "except",
            "finally",
            "for",
            "from",
            "global",
            "if",
            "import",
            "in",
            "is",
            "lambda",
            "nonlocal",
            "not",
            "or",
            "pass",
            "raise",
            "return",
            "try",
            "while",
            "with",
            "yield",
            "print",
            "len",
            "range",
            "enumerate",
            "zip",
            "map",
            "filter",
            "sorted",
            "list",
            "dict",
            "set",
            "tuple",
            "str",
            "int",
            "float",
            "bool",
            "type",
            "isinstance",
            "hasattr",
            "getattr",
            "setattr",
            "open",
            "input",
        ],
        "javascript" | "js" | "node" => &[
            "async",
            "await",
            "break",
            "case",
            "catch",
            "class",
            "const",
            "continue",
            "debugger",
            "default",
            "delete",
            "do",
            "else",
            "export",
            "extends",
            "false",
            "finally",
            "for",
            "function",
            "if",
            "import",
            "in",
            "instanceof",
            "let",
            "new",
            "null",
            "of",
            "return",
            "static",
            "super",
            "switch",
            "this",
            "throw",
            "true",
            "try",
            "typeof",
            "undefined",
            "var",
            "void",
            "while",
            "with",
            "yield",
            "console",
            "require",
            "module",
            "process",
            "Promise",
            "Array",
            "Object",
            "String",
            "Number",
            "Boolean",
            "Math",
            "JSON",
            "Date",
            "RegExp",
            "Map",
            "Set",
        ],
        "typescript" | "ts" => &[
            "abstract",
            "any",
            "as",
            "async",
            "await",
            "boolean",
            "break",
            "case",
            "catch",
            "class",
            "const",
            "continue",
            "debugger",
            "declare",
            "default",
            "delete",
            "do",
            "else",
            "enum",
            "export",
            "extends",
            "false",
            "finally",
            "for",
            "from",
            "function",
            "get",
            "if",
            "implements",
            "import",
            "in",
            "infer",
            "instanceof",
            "interface",
            "is",
            "keyof",
            "let",
            "module",
            "namespace",
            "never",
            "new",
            "null",
            "number",
            "object",
            "of",
            "private",
            "protected",
            "public",
            "readonly",
            "return",
            "set",
            "static",
            "string",
            "super",
            "switch",
            "symbol",
            "this",
            "throw",
            "true",
            "try",
            "type",
            "typeof",
            "undefined",
            "unique",
            "unknown",
            "var",
            "void",
            "while",
            "with",
            "yield",
        ],
        "rust" | "rs" => &[
            "as",
            "async",
            "await",
            "break",
            "const",
            "continue",
            "crate",
            "dyn",
            "else",
            "enum",
            "extern",
            "false",
            "fn",
            "for",
            "if",
            "impl",
            "in",
            "let",
            "loop",
            "match",
            "mod",
            "move",
            "mut",
            "pub",
            "ref",
            "return",
            "self",
            "Self",
            "static",
            "struct",
            "super",
            "trait",
            "true",
            "type",
            "unsafe",
            "use",
            "where",
            "while",
            "println!",
            "eprintln!",
            "format!",
            "vec!",
            "String",
            "Vec",
            "Option",
            "Result",
            "Some",
            "None",
            "Ok",
            "Err",
        ],
        "go" | "golang" => &[
            "break",
            "case",
            "chan",
            "const",
            "continue",
            "default",
            "defer",
            "else",
            "fallthrough",
            "for",
            "func",
            "go",
            "goto",
            "if",
            "import",
            "interface",
            "map",
            "package",
            "range",
            "return",
            "select",
            "struct",
            "switch",
            "type",
            "var",
            "fmt",
            "Println",
            "Printf",
            "Sprintf",
            "errors",
            "strings",
            "strconv",
        ],
        "ruby" | "rb" => &[
            "alias",
            "and",
            "begin",
            "break",
            "case",
            "class",
            "def",
            "defined?",
            "do",
            "else",
            "elsif",
            "end",
            "ensure",
            "false",
            "for",
            "if",
            "in",
            "module",
            "next",
            "nil",
            "not",
            "or",
            "redo",
            "rescue",
            "retry",
            "return",
            "self",
            "super",
            "then",
            "true",
            "undef",
            "unless",
            "until",
            "when",
            "while",
            "yield",
            "puts",
            "print",
            "require",
            "require_relative",
        ],
        "java" => &[
            "abstract",
            "assert",
            "boolean",
            "break",
            "byte",
            "case",
            "catch",
            "char",
            "class",
            "const",
            "continue",
            "default",
            "do",
            "double",
            "else",
            "enum",
            "extends",
            "final",
            "finally",
            "float",
            "for",
            "goto",
            "if",
            "implements",
            "import",
            "instanceof",
            "int",
            "interface",
            "long",
            "native",
            "new",
            "package",
            "private",
            "protected",
            "public",
            "return",
            "short",
            "static",
            "strictfp",
            "super",
            "switch",
            "synchronized",
            "this",
            "throw",
            "throws",
            "transient",
            "try",
            "void",
            "volatile",
            "while",
            "System",
            "String",
        ],
        _ => &[],
    }
}

fn complete_file_path(partial: &str) -> Vec<Pair> {
    let (dir_part, file_prefix) = if let Some(sep_pos) = partial.rfind('/') {
        (&partial[..=sep_pos], &partial[sep_pos + 1..])
    } else {
        ("", partial)
    };

    let search_dir = if dir_part.is_empty() { "." } else { dir_part };

    let mut results = Vec::new();
    if let Ok(entries) = std::fs::read_dir(search_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue; // skip dotfiles
            }
            if name.starts_with(file_prefix) {
                let full = format!("{dir_part}{name}");
                let display = if entry.path().is_dir() {
                    format!("{name}/")
                } else {
                    name.clone()
                };
                results.push(Pair {
                    display,
                    replacement: full,
                });
            }
        }
    }
    results
}

impl Completer for ReplHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let line_up_to = &line[..pos];

        // Meta command completion
        if line_up_to.starts_with(':') {
            // File path completion for :load and :run
            if let Some(rest) = line_up_to
                .strip_prefix(":load ")
                .or_else(|| line_up_to.strip_prefix(":run "))
                .or_else(|| line_up_to.strip_prefix(":save "))
            {
                let start = pos - rest.len();
                return Ok((start, complete_file_path(rest)));
            }

            let candidates: Vec<Pair> = META_COMMANDS
                .iter()
                .filter(|cmd| cmd.starts_with(line_up_to))
                .map(|cmd| Pair {
                    display: cmd.to_string(),
                    replacement: cmd.to_string(),
                })
                .collect();
            return Ok((0, candidates));
        }

        // Find the word being typed
        let word_start = line_up_to
            .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '!')
            .map(|i| i + 1)
            .unwrap_or(0);
        let prefix = &line_up_to[word_start..];

        if prefix.is_empty() {
            return Ok((pos, Vec::new()));
        }

        let mut candidates: Vec<Pair> = Vec::new();

        // Language keywords
        for kw in language_keywords(&self.language_id) {
            if kw.starts_with(prefix) {
                candidates.push(Pair {
                    display: kw.to_string(),
                    replacement: kw.to_string(),
                });
            }
        }

        // Session variables
        for var in &self.session_vars {
            if var.starts_with(prefix) && !candidates.iter().any(|c| c.replacement == *var) {
                candidates.push(Pair {
                    display: var.clone(),
                    replacement: var.clone(),
                });
            }
        }

        Ok((word_start, candidates))
    }
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

    let lang_count = registry.known_languages().len();
    let mut state = ReplState::new(initial_language, registry, detect_enabled)?;

    println!(
        "\x1b[1mrun\x1b[0m \x1b[2mv{} — {}+ languages. Type :help for commands.\x1b[0m",
        env!("CARGO_PKG_VERSION"),
        lang_count
    );
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
                        state.history_entries.push(trimmed.to_string());
                        state.execute_snippet(trimmed)?;
                        if let Some(helper) = editor.helper_mut() {
                            helper.update_session_vars(state.session_var_names());
                        }
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
                state.history_entries.push(trimmed.to_string());
                state.execute_snippet(trimmed)?;
                if let Some(helper) = editor.helper_mut() {
                    helper.update_session_vars(state.session_var_names());
                }
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
    defined_names: HashSet<String>,
    history_entries: Vec<String>,
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
    let mut saw_block_header = false;
    let mut has_body_after_header = false;

    for line in code.lines() {
        let trimmed = line.trim_end();
        if trimmed.trim().is_empty() {
            continue;
        }
        last_nonempty = Some(trimmed);
        if is_python_block_header(trimmed.trim()) {
            saw_block_header = true;
            has_body_after_header = false;
        } else if saw_block_header {
            has_body_after_header = true;
        }
    }

    if !saw_block_header {
        return false;
    }

    // A blank line terminates a block
    if code.ends_with("\n\n") {
        return false;
    }

    // If we have a header but no body yet, we need more input
    if !has_body_after_header {
        return true;
    }

    // If the last line is still indented, we're still inside the block
    if let Some(last) = last_nonempty
        && (last.starts_with(' ') || last.starts_with('\t'))
    {
        return true;
    }

    false
}

/// Check if a trimmed Python line is a block header (def, class, if, for, etc.)
/// rather than a line that just happens to end with `:` (dict literal, slice, etc.)
fn is_python_block_header(line: &str) -> bool {
    if !line.ends_with(':') {
        return false;
    }
    let lowered = line.to_ascii_lowercase();
    const BLOCK_KEYWORDS: &[&str] = &[
        "def ",
        "class ",
        "if ",
        "elif ",
        "else:",
        "for ",
        "while ",
        "try:",
        "except",
        "finally:",
        "with ",
        "async def ",
        "async for ",
        "async with ",
    ];
    BLOCK_KEYWORDS.iter().any(|kw| lowered.starts_with(kw))
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
    let mut in_backtick = false;
    let mut in_block_comment = false;
    let mut escape = false;

    let chars: Vec<char> = code.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let ch = chars[i];

        if escape {
            escape = false;
            i += 1;
            continue;
        }

        // Inside block comment /* ... */
        if in_block_comment {
            if ch == '*' && i + 1 < len && chars[i + 1] == '/' {
                in_block_comment = false;
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }

        if in_single {
            if ch == '\\' {
                escape = true;
            } else if ch == '\'' {
                in_single = false;
            }
            i += 1;
            continue;
        }
        if in_double {
            if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_double = false;
            }
            i += 1;
            continue;
        }
        if in_backtick {
            if ch == '\\' {
                escape = true;
            } else if ch == '`' {
                in_backtick = false;
            }
            i += 1;
            continue;
        }

        // Check for line comments (// and #)
        if ch == '/' && i + 1 < len && chars[i + 1] == '/' {
            // Skip rest of line
            while i < len && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if ch == '#' {
            // Python/Ruby/etc. line comment - skip rest of line
            while i < len && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        // Check for block comments /* ... */
        if ch == '/' && i + 1 < len && chars[i + 1] == '*' {
            in_block_comment = true;
            i += 2;
            continue;
        }

        match ch {
            '\'' => in_single = true,
            '"' => in_double = true,
            '`' => in_backtick = true,
            '(' => paren += 1,
            ')' => paren -= 1,
            '[' => bracket += 1,
            ']' => bracket -= 1,
            '{' => brace += 1,
            '}' => brace -= 1,
            _ => {}
        }

        i += 1;
    }

    paren > 0 || bracket > 0 || brace > 0 || in_block_comment
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
            defined_names: HashSet::new(),
            history_entries: Vec::new(),
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
        let Some(head) = parts.next() else {
            return Ok(false);
        };
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
            "save" => {
                if let Some(token) = parts.next() {
                    let path = Path::new(token);
                    match self.save_session(path) {
                        Ok(count) => println!(
                            "\x1b[2m[saved {count} entries to {}]\x1b[0m",
                            path.display()
                        ),
                        Err(e) => println!("error saving session: {e}"),
                    }
                } else {
                    println!("usage: :save <path>");
                }
                return Ok(false);
            }
            "history" => {
                let limit: usize = parts.next().and_then(|s| s.parse().ok()).unwrap_or(25);
                self.show_history(limit);
                return Ok(false);
            }
            "install" => {
                if let Some(pkg) = parts.next() {
                    self.install_package(pkg);
                } else {
                    println!("usage: :install <package>");
                }
                return Ok(false);
            }
            "bench" => {
                let n: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(10);
                let code = parts.collect::<Vec<_>>().join(" ");
                if code.is_empty() {
                    println!("usage: :bench [N] <code>");
                    println!("  Runs <code> N times (default: 10) and reports timing stats.");
                } else {
                    self.bench_code(&code, n)?;
                }
                return Ok(false);
            }
            "type" | "which" => {
                let lang = &self.current_language;
                println!(
                    "\x1b[1m{}\x1b[0m \x1b[2m({})\x1b[0m",
                    lang.canonical_id(),
                    if self.sessions.contains_key(lang.canonical_id()) {
                        "session active"
                    } else {
                        "no session"
                    }
                );
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
        if self.detect_enabled
            && let Some(detected) = crate::detect::detect_language_from_snippet(code)
            && detected != self.current_language.canonical_id()
        {
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

        // Track defined variable names for tab completion
        self.defined_names.extend(extract_defined_names(
            code,
            self.current_language.canonical_id(),
        ));

        let payload = ExecutionPayload::Inline {
            code: code.to_string(),
        };
        self.execute_payload(payload)
    }

    fn session_var_names(&self) -> Vec<String> {
        self.defined_names.iter().cloned().collect()
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
            ExecutionPayload::File { ref path } => {
                // Read the file and feed it through the session so variables persist
                if self.engine_supports_sessions(&language)? {
                    let code = std::fs::read_to_string(path)
                        .with_context(|| format!("failed to read file: {}", path.display()))?;
                    println!("\x1b[2m[loaded {}]\x1b[0m", path.display());
                    self.eval_in_session(&language, &code)?
                } else {
                    let engine = self
                        .registry
                        .resolve(&language)
                        .context("language engine not found")?;
                    engine.execute(&payload)?
                }
            }
            ExecutionPayload::Stdin { code } => {
                if self.engine_supports_sessions(&language)? {
                    self.eval_in_session(&language, &code)?
                } else {
                    let engine = self
                        .registry
                        .resolve(&language)
                        .context("language engine not found")?;
                    engine.execute(&ExecutionPayload::Stdin { code })?
                }
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

    fn install_package(&self, package: &str) {
        let lang_id = self.current_language.canonical_id();
        if package_install_command(lang_id).is_none() {
            println!("No package manager available for '{lang_id}'.");
            return;
        }

        let Some(mut cmd) = build_install_command(lang_id, package) else {
            println!("Failed to build install command for '{lang_id}'.");
            return;
        };

        println!("\x1b[36m[run]\x1b[0m Installing '{package}' for {lang_id}...");

        match cmd
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()
        {
            Ok(status) if status.success() => {
                println!("\x1b[32m[run]\x1b[0m Successfully installed '{package}'");
            }
            Ok(_) => {
                println!("\x1b[31m[run]\x1b[0m Failed to install '{package}'");
            }
            Err(e) => {
                println!("\x1b[31m[run]\x1b[0m Error running package manager: {e}");
            }
        }
    }

    fn bench_code(&mut self, code: &str, iterations: u32) -> Result<()> {
        let language = self.current_language.clone();

        // Warmup
        let warmup = self.eval_in_session(&language, code)?;
        if !warmup.success() {
            println!("\x1b[31mError:\x1b[0m Code failed during warmup");
            if !warmup.stderr.is_empty() {
                print!("{}", warmup.stderr);
            }
            return Ok(());
        }
        println!("\x1b[2m  warmup: {}ms\x1b[0m", warmup.duration.as_millis());

        let mut times: Vec<f64> = Vec::with_capacity(iterations as usize);
        for i in 0..iterations {
            let outcome = self.eval_in_session(&language, code)?;
            let ms = outcome.duration.as_secs_f64() * 1000.0;
            times.push(ms);
            if i < 3 || i == iterations - 1 {
                println!("\x1b[2m  run {}: {:.2}ms\x1b[0m", i + 1, ms);
            }
        }

        times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let total: f64 = times.iter().sum();
        let avg = total / times.len() as f64;
        let min = times.first().copied().unwrap_or(0.0);
        let max = times.last().copied().unwrap_or(0.0);
        let median = if times.len().is_multiple_of(2) && times.len() >= 2 {
            (times[times.len() / 2 - 1] + times[times.len() / 2]) / 2.0
        } else {
            times[times.len() / 2]
        };
        let variance: f64 =
            times.iter().map(|t| (t - avg).powi(2)).sum::<f64>() / times.len() as f64;
        let stddev = variance.sqrt();

        println!();
        println!("\x1b[1mResults ({iterations} runs):\x1b[0m");
        println!("  min:    \x1b[32m{min:.2}ms\x1b[0m");
        println!("  max:    \x1b[33m{max:.2}ms\x1b[0m");
        println!("  avg:    \x1b[36m{avg:.2}ms\x1b[0m");
        println!("  median: \x1b[36m{median:.2}ms\x1b[0m");
        println!("  stddev: {stddev:.2}ms");
        Ok(())
    }

    fn save_session(&self, path: &Path) -> Result<usize> {
        use std::io::Write;
        let mut file = std::fs::File::create(path)
            .with_context(|| format!("failed to create {}", path.display()))?;
        let count = self.history_entries.len();
        for entry in &self.history_entries {
            writeln!(file, "{entry}")?;
        }
        Ok(count)
    }

    fn show_history(&self, limit: usize) {
        let entries = &self.history_entries;
        let start = entries.len().saturating_sub(limit);
        if entries.is_empty() {
            println!("\x1b[2m(no history)\x1b[0m");
            return;
        }
        for (i, entry) in entries[start..].iter().enumerate() {
            let num = start + i + 1;
            // Show multi-line entries with continuation indicator
            let first_line = entry.lines().next().unwrap_or(entry);
            let is_multiline = entry.contains('\n');
            if is_multiline {
                println!("\x1b[2m[{num:>4}]\x1b[0m {first_line} \x1b[2m(...)\x1b[0m");
            } else {
                println!("\x1b[2m[{num:>4}]\x1b[0m {entry}");
            }
        }
    }

    fn print_help(&self) {
        println!("\x1b[1mCommands\x1b[0m");
        println!("  \x1b[36m:help\x1b[0m                 \x1b[2mShow this help\x1b[0m");
        println!("  \x1b[36m:lang\x1b[0m <id>            \x1b[2mSwitch language\x1b[0m");
        println!("  \x1b[36m:languages\x1b[0m            \x1b[2mList available languages\x1b[0m");
        println!(
            "  \x1b[36m:detect\x1b[0m on|off        \x1b[2mToggle auto language detection\x1b[0m"
        );
        println!(
            "  \x1b[36m:reset\x1b[0m                \x1b[2mClear current session state\x1b[0m"
        );
        println!("  \x1b[36m:load\x1b[0m <path>          \x1b[2mLoad and execute a file\x1b[0m");
        println!(
            "  \x1b[36m:save\x1b[0m <path>          \x1b[2mSave session history to file\x1b[0m"
        );
        println!(
            "  \x1b[36m:history\x1b[0m [n]          \x1b[2mShow last n entries (default: 25)\x1b[0m"
        );
        println!(
            "  \x1b[36m:install\x1b[0m <pkg>        \x1b[2mInstall a package for current language\x1b[0m"
        );
        println!(
            "  \x1b[36m:bench\x1b[0m [N] <code>     \x1b[2mBenchmark code N times (default: 10)\x1b[0m"
        );
        println!(
            "  \x1b[36m:type\x1b[0m                 \x1b[2mShow current language and session status\x1b[0m"
        );
        println!("  \x1b[36m:exit\x1b[0m                 \x1b[2mLeave the REPL\x1b[0m");
        println!("\x1b[2mLanguage shortcuts: :py, :js, :rs, :go, :cpp, :java, ...\x1b[0m");
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
        eprint!(
            "\x1b[31m{}\x1b[0m",
            ensure_trailing_newline(&outcome.stderr)
        );
    }

    let millis = outcome.duration.as_millis();
    if let Some(code) = outcome.exit_code
        && code != 0
    {
        println!("\x1b[2m[exit {code}] {}\x1b[0m", format_duration(millis));
        return;
    }

    // Show execution timing
    if millis > 0 {
        println!("\x1b[2m{}\x1b[0m", format_duration(millis));
    }
}

fn format_duration(millis: u128) -> String {
    if millis >= 60_000 {
        let mins = millis / 60_000;
        let secs = (millis % 60_000) / 1000;
        format!("{mins}m {secs}s")
    } else if millis >= 1000 {
        let secs = millis as f64 / 1000.0;
        format!("{secs:.2}s")
    } else {
        format!("{millis}ms")
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

/// Extract variable/function/class names defined in a code snippet for tab completion.
fn extract_defined_names(code: &str, language_id: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in code.lines() {
        let trimmed = line.trim();
        match language_id {
            "python" | "py" | "python3" | "py3" => {
                // x = ..., def foo(...), class Bar:, import x, from x import y
                if let Some(rest) = trimmed.strip_prefix("def ") {
                    if let Some(name) = rest.split('(').next() {
                        let n = name.trim();
                        if !n.is_empty() {
                            names.push(n.to_string());
                        }
                    }
                } else if let Some(rest) = trimmed.strip_prefix("class ") {
                    let name = rest.split(['(', ':']).next().unwrap_or("").trim();
                    if !name.is_empty() {
                        names.push(name.to_string());
                    }
                } else if let Some(rest) = trimmed.strip_prefix("import ") {
                    for part in rest.split(',') {
                        let name = if let Some(alias) = part.split(" as ").nth(1) {
                            alias.trim()
                        } else {
                            part.trim().split('.').next_back().unwrap_or("")
                        };
                        if !name.is_empty() {
                            names.push(name.to_string());
                        }
                    }
                } else if trimmed.starts_with("from ") && trimmed.contains("import ") {
                    if let Some(imports) = trimmed.split("import ").nth(1) {
                        for part in imports.split(',') {
                            let name = if let Some(alias) = part.split(" as ").nth(1) {
                                alias.trim()
                            } else {
                                part.trim()
                            };
                            if !name.is_empty() {
                                names.push(name.to_string());
                            }
                        }
                    }
                } else if let Some(eq_pos) = trimmed.find('=') {
                    let lhs = &trimmed[..eq_pos];
                    if !lhs.contains('(')
                        && !lhs.contains('[')
                        && !trimmed[eq_pos..].starts_with("==")
                    {
                        for part in lhs.split(',') {
                            let name = part.trim().split(':').next().unwrap_or("").trim();
                            if !name.is_empty()
                                && name.chars().all(|c| c.is_alphanumeric() || c == '_')
                            {
                                names.push(name.to_string());
                            }
                        }
                    }
                }
            }
            "javascript" | "js" | "node" | "typescript" | "ts" => {
                // let/const/var x = ..., function foo(...), class Bar
                for prefix in ["let ", "const ", "var "] {
                    if let Some(rest) = trimmed.strip_prefix(prefix) {
                        let name = rest.split(['=', ':', ';', ' ']).next().unwrap_or("").trim();
                        if !name.is_empty() {
                            names.push(name.to_string());
                        }
                    }
                }
                if let Some(rest) = trimmed.strip_prefix("function ") {
                    let name = rest.split('(').next().unwrap_or("").trim();
                    if !name.is_empty() {
                        names.push(name.to_string());
                    }
                } else if let Some(rest) = trimmed.strip_prefix("class ") {
                    let name = rest.split(['{', ' ']).next().unwrap_or("").trim();
                    if !name.is_empty() {
                        names.push(name.to_string());
                    }
                }
            }
            "rust" | "rs" => {
                for prefix in ["let ", "let mut "] {
                    if let Some(rest) = trimmed.strip_prefix(prefix) {
                        let name = rest.split(['=', ':', ';', ' ']).next().unwrap_or("").trim();
                        if !name.is_empty() {
                            names.push(name.to_string());
                        }
                    }
                }
                if let Some(rest) = trimmed.strip_prefix("fn ") {
                    let name = rest.split(['(', '<']).next().unwrap_or("").trim();
                    if !name.is_empty() {
                        names.push(name.to_string());
                    }
                } else if let Some(rest) = trimmed.strip_prefix("struct ") {
                    let name = rest.split(['{', '(', '<', ' ']).next().unwrap_or("").trim();
                    if !name.is_empty() {
                        names.push(name.to_string());
                    }
                }
            }
            _ => {
                // Generic: catch x = ... assignments
                if let Some(eq_pos) = trimmed.find('=') {
                    let lhs = trimmed[..eq_pos].trim();
                    if !lhs.is_empty()
                        && !trimmed[eq_pos..].starts_with("==")
                        && lhs
                            .chars()
                            .all(|c| c.is_alphanumeric() || c == '_' || c == ' ')
                        && let Some(name) = lhs.split_whitespace().last()
                    {
                        names.push(name.to_string());
                    }
                }
            }
        }
    }
    names
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
    fn python_dict_literal_colon_does_not_trigger_block() {
        let mut p = PendingInput::new();
        p.push_line("x = {'key': 'value'}");
        assert!(
            !p.needs_more_input("python"),
            "dict literal should not trigger multi-line"
        );
    }

    #[test]
    fn python_class_block_needs_body() {
        let mut p = PendingInput::new();
        p.push_line("class Foo:");
        assert!(p.needs_more_input("python"));
        p.push_line("    pass");
        assert!(p.needs_more_input("python")); // still indented
        p.push_line(""); // blank line ends
        assert!(!p.needs_more_input("python"));
    }

    #[test]
    fn python_if_block_with_dedented_body_is_complete() {
        let mut p = PendingInput::new();
        p.push_line("if True:");
        assert!(p.needs_more_input("python"));
        p.push_line("    print('yes')");
        assert!(p.needs_more_input("python"));
        p.push_line(""); // blank line terminates
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
