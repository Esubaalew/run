use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

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
};
use crate::highlight;
use crate::language::LanguageSpec;
use crate::output;

const HISTORY_FILE: &str = ".run_history";
const BOOKMARKS_FILE: &str = ".run_bookmarks";
const REPL_CONFIG_FILE: &str = ".run_repl_config";
const MAX_DIR_STACK: usize = 20;

/// Exception/stderr display mode for the REPL.
#[derive(Clone, Copy, PartialEq, Eq)]
enum XMode {
    /// First line of stderr only (compact).
    Plain,
    /// First few lines (e.g. 5) for context.
    Context,
    /// Full stderr (default).
    Verbose,
}

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
    ":! ",
    ":!! ",
    ":help",
    ":help ",
    ":? ",
    ":debug",
    ":debug ",
    ":commands",
    ":quickref",
    ":exit",
    ":quit",
    ":languages",
    ":lang ",
    ":detect ",
    ":reset",
    ":cd ",
    ":cd -b ",
    ":dhist",
    ":bookmark ",
    ":bookmark -l",
    ":bookmark -d ",
    ":env",
    ":last",
    ":load ",
    ":edit",
    ":edit ",
    ":run ",
    ":logstart",
    ":logstart ",
    ":logstop",
    ":logstate",
    ":macro ",
    ":macro run ",
    ":time ",
    ":who",
    ":whos",
    ":whos ",
    ":xmode",
    ":xmode ",
    ":config",
    ":config ",
    ":paste",
    ":end",
    ":precision",
    ":precision ",
    ":save ",
    ":history",
    ":install ",
    ":bench ",
    ":type",
];

/// (name without colon, one-line description) for :help, :commands, :quickref, :help :cmd
const CMD_HELP: &[(&str, &str)] = &[
    ("help", "Show this help"),
    (
        "?",
        "Show doc/source for name (e.g. :? print); Python session only",
    ),
    (
        "debug",
        "Run last snippet or :debug CODE under debugger (Python: pdb)",
    ),
    ("lang", "Switch language"),
    ("languages", "List available languages"),
    ("versions", "Show toolchain versions"),
    ("detect", "Toggle auto language detection"),
    ("reset", "Clear current session state"),
    (
        "cd",
        "Change directory; :cd - = previous, :cd -b <name> = bookmark",
    ),
    ("dhist", "Directory history (default 10)"),
    ("bookmark", "Save bookmark; -l list, -d <name> delete"),
    ("env", "List env, get VAR, or set VAR=val"),
    ("load", "Load and execute a file or http(s) URL"),
    ("last", "Print last execution stdout"),
    ("edit", "Open $EDITOR; on save, execute in current session"),
    ("run", "Load file/URL or run macro by name"),
    (
        "logstart",
        "Start logging input to file (default: run_log.txt)",
    ),
    ("logstop", "Stop logging"),
    ("logstate", "Show whether logging and path"),
    (
        "macro",
        "Save history range as macro; :macro run NAME to run",
    ),
    ("time", "Run code once and print elapsed time"),
    ("who", "List names tracked in current session"),
    ("whos", "Like :who with optional name filter"),
    ("save", "Save session history to file"),
    (
        "history",
        "Show history; -g PATTERN, -f FILE, 4-6 or 4- or -6",
    ),
    ("install", "Install a package for current language"),
    ("bench", "Benchmark code N times (default: 10)"),
    ("type", "Show current language and session status"),
    ("!", "Run shell command (inherit stdout/stderr)"),
    ("!!", "Run shell command and print captured output"),
    ("exit", "Leave the REPL"),
    ("quit", "Leave the REPL"),
    (
        "xmode",
        "Exception display: plain (first line) | context (5 lines) | verbose (full)",
    ),
    (
        "config",
        "Get/set REPL config (detect, xmode); persists in ~/.run_repl_config",
    ),
    (
        "paste",
        "Paste mode: collect lines until :end or Ctrl-D, then execute (strip >>> / ...)",
    ),
    (
        "end",
        "End paste mode and execute buffer (only in paste mode)",
    ),
    (
        "precision",
        "Float display precision (0–32) for last result; :precision N to set, persists in config",
    ),
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
        let mut pending_indent: Option<String> = None;
        if let Some(p) = pending.as_ref()
            && state.current_language().canonical_id() == "python"
        {
            let indent = python_prompt_indent(p.buffer());
            if !indent.is_empty() {
                pending_indent = Some(indent);
            }
        }

        if let Some(helper) = editor.helper_mut() {
            helper.update_language(state.current_language().canonical_id().to_string());
        }

        let line_result = match pending_indent.as_deref() {
            Some(indent) => editor.readline_with_initial(&prompt, (indent, "")),
            None => editor.readline(&prompt),
        };

        match line_result {
            Ok(line) => {
                let raw = line.trim_end_matches(['\r', '\n']);

                if let Some(p) = pending.as_mut() {
                    if raw.trim() == ":cancel" {
                        pending = None;
                        continue;
                    }

                    p.push_line_auto_with_indent(
                        state.current_language().canonical_id(),
                        raw,
                        pending_indent.as_deref(),
                    );
                    if p.needs_more_input(state.current_language().canonical_id()) {
                        continue;
                    }

                    let code = p.take();
                    pending = None;
                    let trimmed = code.trim_end();
                    if !trimmed.is_empty() {
                        let _ = editor.add_history_entry(trimmed);
                        state.history_entries.push(trimmed.to_string());
                        state.log_input(trimmed);
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

                if state.paste_buffer.is_some() {
                    if raw.trim() == ":end" {
                        let lines = state.paste_buffer.take().unwrap();
                        let code = strip_paste_prompts(&lines);
                        if !code.trim().is_empty() {
                            let _ = editor.add_history_entry(code.trim());
                            state.history_entries.push(code.trim().to_string());
                            state.log_input(code.trim());
                            if let Err(e) = state.execute_snippet(code.trim()) {
                                println!("\x1b[31m[run]\x1b[0m {e}");
                            }
                            if let Some(helper) = editor.helper_mut() {
                                helper.update_session_vars(state.session_var_names());
                            }
                        }
                        println!("\x1b[2m[paste done]\x1b[0m");
                    } else {
                        state.paste_buffer.as_mut().unwrap().push(raw.to_string());
                    }
                    continue;
                }

                if raw.trim_start().starts_with(':') {
                    let trimmed = raw.trim();
                    let _ = editor.add_history_entry(trimmed);
                    state.log_input(trimmed);
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
                state.log_input(trimmed);
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
                if let Some(lines) = state.paste_buffer.take() {
                    let code = strip_paste_prompts(&lines);
                    if !code.trim().is_empty() {
                        let _ = editor.add_history_entry(code.trim());
                        state.history_entries.push(code.trim().to_string());
                        state.log_input(code.trim());
                        if let Err(e) = state.execute_snippet(code.trim()) {
                            println!("\x1b[31m[run]\x1b[0m {e}");
                        }
                    }
                    println!("\x1b[2m[paste done]\x1b[0m");
                }
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
    dir_stack: Vec<PathBuf>,
    bookmarks: HashMap<String, PathBuf>,
    log_path: Option<PathBuf>,
    macros: HashMap<String, String>,
    xmode: XMode,
    /// When Some, we are in paste mode; lines are collected until :end or Ctrl-D.
    paste_buffer: Option<Vec<String>>,
    /// Float display precision for last result (when we show it). None = default.
    precision: Option<u32>,
    /// In[n] counter for numbered prompts (e.g. python [3]>>>).
    in_count: usize,
    /// Last execution stdout, for :last.
    last_stdout: Option<String>,
    /// Whether to show [n] in prompt (config: numbered_prompts).
    numbered_prompts: bool,
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

    fn buffer(&self) -> &str {
        &self.buf
    }

    fn push_line(&mut self, line: &str) {
        self.buf.push_str(line);
        self.buf.push('\n');
    }

    #[cfg(test)]
    fn push_line_auto(&mut self, language_id: &str, line: &str) {
        self.push_line_auto_with_indent(language_id, line, None);
    }

    fn push_line_auto_with_indent(
        &mut self,
        language_id: &str,
        line: &str,
        expected_indent: Option<&str>,
    ) {
        match language_id {
            "python" | "py" | "python3" | "py3" => {
                let adjusted = python_auto_indent_with_expected(line, &self.buf, expected_indent);
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
    if line.starts_with('#') {
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
    python_auto_indent_with_expected(line, existing, None)
}

fn python_auto_indent_with_expected(
    line: &str,
    existing: &str,
    expected_indent: Option<&str>,
) -> String {
    let trimmed = line.trim_end_matches(['\r', '\n']);
    let raw = trimmed;
    if raw.trim().is_empty() {
        return raw.to_string();
    }

    let (raw_indent, raw_content) = split_indent(raw);

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
    let prev_indent = prev
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect::<String>();

    let lowered = raw.trim().to_ascii_lowercase();
    let is_dedent_keyword = lowered.starts_with("else:")
        || lowered.starts_with("elif ")
        || lowered.starts_with("except")
        || lowered.starts_with("finally:")
        || lowered.starts_with("return")
        || lowered.starts_with("yield")
        || lowered == "return"
        || lowered == "yield";
    let suggested = if lowered.starts_with("else:")
        || lowered.starts_with("elif ")
        || lowered.starts_with("except")
        || lowered.starts_with("finally:")
    {
        if prev_indent.is_empty() {
            None
        } else {
            Some(python_dedent_one_level(&prev_indent))
        }
    } else if lowered.starts_with("return")
        || lowered.starts_with("yield")
        || lowered == "return"
        || lowered == "yield"
    {
        python_last_def_indent(existing).map(|indent| format!("{indent}    "))
    } else if is_python_block_header(prev_trimmed.trim()) && prev_trimmed.ends_with(':') {
        Some(format!("{prev_indent}    "))
    } else if !prev_indent.is_empty() {
        Some(prev_indent)
    } else {
        None
    };

    if let Some(indent) = suggested {
        if let Some(expected) = expected_indent {
            if raw_indent.len() < expected.len() {
                return raw.to_string();
            }
            if is_dedent_keyword
                && raw_indent.len() == expected.len()
                && raw_indent.len() > indent.len()
            {
                return format!("{indent}{raw_content}");
            }
        }
        if raw_indent.len() < indent.len() {
            return format!("{indent}{raw_content}");
        }
    }

    raw.to_string()
}

fn python_prompt_indent(existing: &str) -> String {
    if existing.trim().is_empty() {
        return String::new();
    }
    let adjusted = python_auto_indent("x", existing);
    let (indent, _content) = split_indent(&adjusted);
    indent
}

fn python_dedent_one_level(indent: &str) -> String {
    if indent.is_empty() {
        return String::new();
    }
    if let Some(stripped) = indent.strip_suffix('\t') {
        return stripped.to_string();
    }
    let mut trimmed = indent.to_string();
    let mut removed = 0usize;
    while removed < 4 && trimmed.ends_with(' ') {
        trimmed.pop();
        removed += 1;
    }
    trimmed
}

fn python_last_def_indent(existing: &str) -> Option<String> {
    for line in existing.lines().rev() {
        let trimmed = line.trim_end();
        if trimmed.trim().is_empty() {
            continue;
        }
        let lowered = trimmed.trim_start().to_ascii_lowercase();
        if lowered.starts_with("def ") || lowered.starts_with("async def ") {
            let indent = line
                .chars()
                .take_while(|c| *c == ' ' || *c == '\t')
                .collect::<String>();
            return Some(indent);
        }
    }
    None
}

fn split_indent(line: &str) -> (String, &str) {
    let mut idx = 0;
    for (i, ch) in line.char_indices() {
        if ch == ' ' || ch == '\t' {
            idx = i + ch.len_utf8();
        } else {
            break;
        }
    }
    (line[..idx].to_string(), &line[idx..])
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
        let bookmarks = load_bookmarks().unwrap_or_default();
        let mut state = Self {
            registry,
            sessions: HashMap::new(),
            current_language: initial_language,
            detect_enabled,
            defined_names: HashSet::new(),
            history_entries: Vec::new(),
            dir_stack: Vec::new(),
            bookmarks,
            log_path: None,
            macros: HashMap::new(),
            xmode: XMode::Verbose,
            paste_buffer: None,
            precision: None,
            in_count: 0,
            last_stdout: None,
            numbered_prompts: false,
        };
        if let Ok(cfg) = load_repl_config() {
            if let Some(v) = cfg.get("detect") {
                state.detect_enabled = matches!(v.to_lowercase().as_str(), "on" | "true" | "1");
            }
            if let Some(v) = cfg.get("xmode") {
                state.xmode = match v.to_lowercase().as_str() {
                    "plain" => XMode::Plain,
                    "context" => XMode::Context,
                    _ => XMode::Verbose,
                };
            }
            if let Some(v) = cfg.get("precision")
                && let Ok(n) = v.parse::<u32>()
            {
                state.precision = Some(n.min(32));
            }
            if let Some(v) = cfg.get("numbered_prompts") {
                state.numbered_prompts = matches!(v.to_lowercase().as_str(), "on" | "true" | "1");
            }
        }
        state.ensure_current_language()?;
        Ok(state)
    }

    fn current_language(&self) -> &LanguageSpec {
        &self.current_language
    }

    fn prompt(&self) -> String {
        if self.numbered_prompts {
            format!(
                "{} [{}]>>> ",
                self.current_language.canonical_id(),
                self.in_count + 1
            )
        } else {
            format!("{}>>> ", self.current_language.canonical_id())
        }
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

        // Shell escape :!! (capture) and :! (inherit)
        if let Some(stripped) = command.strip_prefix("!!") {
            let shell_cmd = stripped.trim_start();
            if shell_cmd.is_empty() {
                println!("usage: :!! <cmd>");
            } else {
                run_shell(shell_cmd, true);
            }
            return Ok(false);
        }
        if let Some(stripped) = command.strip_prefix('!') {
            let shell_cmd = stripped.trim_start();
            if shell_cmd.is_empty() {
                println!("usage: :! <cmd>");
            } else {
                run_shell(shell_cmd, false);
            }
            return Ok(false);
        }

        let mut parts = command.split_whitespace();
        let Some(head) = parts.next() else {
            return Ok(false);
        };
        match head {
            "exit" | "quit" => return Ok(true),
            "help" => {
                if let Some(arg) = parts.next() {
                    Self::print_cmd_help(arg);
                } else {
                    self.print_help();
                }
                return Ok(false);
            }
            "commands" => {
                Self::print_commands_machine();
                return Ok(false);
            }
            "quickref" => {
                Self::print_quickref();
                return Ok(false);
            }
            "?" => {
                let expr = parts.collect::<Vec<_>>().join(" ").trim().to_string();
                if expr.is_empty() {
                    println!(
                        "usage: :? <name>  — show doc/source for <name> (e.g. :? print). Supported in Python session."
                    );
                } else if !expr
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
                {
                    println!(
                        "\x1b[31m[run]\x1b[0m :? only accepts names (letters, digits, dots, underscores)."
                    );
                } else if let Err(e) = self.run_introspect(&expr) {
                    println!("\x1b[31m[run]\x1b[0m {e}");
                }
                return Ok(false);
            }
            "debug" => {
                let rest: String = parts.collect::<Vec<_>>().join(" ");
                let code = rest.trim();
                let code = if code.is_empty() {
                    self.history_entries
                        .last()
                        .map(String::as_str)
                        .unwrap_or("")
                } else {
                    code
                };
                if code.is_empty() {
                    println!(
                        "usage: :debug [CODE]  — run last snippet (or CODE) under debugger. Python: pdb."
                    );
                } else if let Err(e) = self.run_debug(code) {
                    println!("\x1b[31m[run]\x1b[0m {e}");
                }
                return Ok(false);
            }
            "languages" => {
                self.print_languages();
                return Ok(false);
            }
            "versions" => {
                if let Some(lang) = parts.next() {
                    let spec = LanguageSpec::new(lang.to_string());
                    if self.registry.resolve(&spec).is_some() {
                        self.print_versions(Some(spec))?;
                    } else {
                        let available = self.registry.known_languages().join(", ");
                        println!(
                            "language '{}' not supported. Available: {available}",
                            spec.canonical_id()
                        );
                    }
                } else {
                    self.print_versions(None)?;
                }
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
            "cd" => {
                let arg = parts.next();
                if let Some("-b") = arg {
                    if let Some(name) = parts.next() {
                        if let Some(path) = self.bookmarks.get(name) {
                            if let Ok(cwd) = std::env::current_dir() {
                                if self.dir_stack.len() < MAX_DIR_STACK {
                                    self.dir_stack.push(cwd);
                                } else {
                                    self.dir_stack.remove(0);
                                    self.dir_stack.push(cwd);
                                }
                            }
                            if std::env::set_current_dir(path).is_ok() {
                                println!("{}", path.display());
                            } else {
                                println!(
                                    "\x1b[31m[run]\x1b[0m cd: {}: no such directory",
                                    path.display()
                                );
                            }
                        } else {
                            println!("\x1b[31m[run]\x1b[0m bookmark '{}' not found", name);
                        }
                    } else {
                        println!("usage: :cd -b <bookmark>");
                    }
                } else if let Some(dir) = arg {
                    if dir == "-" {
                        if let Some(prev) = self.dir_stack.pop() {
                            if std::env::set_current_dir(&prev).is_ok() {
                                println!("{}", prev.display());
                            }
                        } else {
                            println!("\x1b[2m[run]\x1b[0m directory stack empty");
                        }
                    } else {
                        let path = PathBuf::from(dir);
                        if let Ok(cwd) = std::env::current_dir() {
                            if self.dir_stack.len() < MAX_DIR_STACK {
                                self.dir_stack.push(cwd);
                            } else {
                                self.dir_stack.remove(0);
                                self.dir_stack.push(cwd);
                            }
                        }
                        if std::env::set_current_dir(&path).is_ok() {
                            println!("{}", path.display());
                        } else {
                            println!(
                                "\x1b[31m[run]\x1b[0m cd: {}: no such directory",
                                path.display()
                            );
                        }
                    }
                } else if let Ok(cwd) = std::env::current_dir() {
                    println!("{}", cwd.display());
                }
                return Ok(false);
            }
            "dhist" => {
                let n: usize = parts.next().and_then(|s| s.parse().ok()).unwrap_or(10);
                let len = self.dir_stack.len();
                let start = len.saturating_sub(n);
                if self.dir_stack.is_empty() {
                    println!("\x1b[2m(no directory history)\x1b[0m");
                } else {
                    for (i, p) in self.dir_stack[start..].iter().enumerate() {
                        let num = start + i + 1;
                        println!("\x1b[2m[{num:>2}]\x1b[0m {}", p.display());
                    }
                }
                return Ok(false);
            }
            "env" => {
                let a = parts.next();
                let b = parts.next();
                match (a, b) {
                    (None, _) => {
                        let vars: BTreeMap<String, String> = std::env::vars().collect();
                        for (k, v) in vars {
                            println!("{k}={v}");
                        }
                    }
                    (Some(var), None) => {
                        if let Some((k, v)) = var.split_once('=') {
                            unsafe { std::env::set_var(k, v) };
                        } else if let Ok(v) = std::env::var(var) {
                            println!("{v}");
                        }
                    }
                    (Some(var), Some(val)) => {
                        if val == "=" {
                            if let Some(v) = parts.next() {
                                unsafe { std::env::set_var(var, v) };
                            }
                        } else if val.starts_with('=') {
                            unsafe { std::env::set_var(var, val.trim_start_matches('=')) };
                        } else {
                            unsafe { std::env::set_var(var, val) };
                        }
                    }
                }
                return Ok(false);
            }
            "bookmark" => {
                let arg = parts.next();
                match arg {
                    Some("-l") => {
                        if self.bookmarks.is_empty() {
                            println!("\x1b[2m(no bookmarks)\x1b[0m");
                        } else {
                            let mut names: Vec<_> = self.bookmarks.keys().collect();
                            names.sort();
                            for name in names {
                                let path = self.bookmarks.get(name).unwrap();
                                println!("  {name}\t{}", path.display());
                            }
                        }
                    }
                    Some("-d") => {
                        if let Some(name) = parts.next() {
                            if self.bookmarks.remove(name).is_some() {
                                let _ = save_bookmarks(&self.bookmarks);
                                println!("\x1b[2m[removed bookmark '{name}']\x1b[0m");
                            } else {
                                println!("\x1b[31m[run]\x1b[0m bookmark '{}' not found", name);
                            }
                        } else {
                            println!("usage: :bookmark -d <name>");
                        }
                    }
                    Some(name) if !name.starts_with('-') => {
                        let path = parts
                            .next()
                            .map(PathBuf::from)
                            .or_else(|| std::env::current_dir().ok());
                        if let Some(p) = path {
                            if p.is_absolute() {
                                self.bookmarks.insert(name.to_string(), p.clone());
                                let _ = save_bookmarks(&self.bookmarks);
                                println!("\x1b[2m[bookmark '{name}' -> {}]\x1b[0m", p.display());
                            } else {
                                println!("\x1b[31m[run]\x1b[0m bookmark path must be absolute");
                            }
                        } else {
                            println!("\x1b[31m[run]\x1b[0m could not get current directory");
                        }
                    }
                    _ => {
                        println!(
                            "usage: :bookmark <name> [path] | :bookmark -l | :bookmark -d <name>"
                        );
                    }
                }
                return Ok(false);
            }
            "load" | "run" => {
                if let Some(token) = parts.next() {
                    if let Some(code) = self.macros.get(token) {
                        self.execute_payload(ExecutionPayload::Inline {
                            code: code.clone(),
                            args: Vec::new(),
                        })?;
                    } else {
                        let path = if token.starts_with("http://") || token.starts_with("https://")
                        {
                            match fetch_url_to_temp(token) {
                                Ok(p) => p,
                                Err(e) => {
                                    println!("\x1b[31m[run]\x1b[0m fetch failed: {e}");
                                    return Ok(false);
                                }
                            }
                        } else {
                            PathBuf::from(token)
                        };
                        self.execute_payload(ExecutionPayload::File {
                            path,
                            args: Vec::new(),
                        })?;
                    }
                } else {
                    println!("usage: :load <path|url>  or  :run <macro|path|url>");
                }
                return Ok(false);
            }
            "edit" => {
                let path = if let Some(token) = parts.next() {
                    PathBuf::from(token)
                } else {
                    match edit_temp_file() {
                        Ok(p) => p,
                        Err(e) => {
                            println!("\x1b[31m[run]\x1b[0m edit: {e}");
                            return Ok(false);
                        }
                    }
                };
                if run_editor(path.as_path()).is_err() {
                    println!("\x1b[31m[run]\x1b[0m editor failed or $EDITOR not set");
                    return Ok(false);
                }
                if path.exists() {
                    self.execute_payload(ExecutionPayload::File {
                        path,
                        args: Vec::new(),
                    })?;
                }
                return Ok(false);
            }
            "last" => {
                if let Some(ref s) = self.last_stdout {
                    print!("{}", ensure_trailing_newline(s));
                } else {
                    println!("\x1b[2m(no last output)\x1b[0m");
                }
                return Ok(false);
            }
            "logstart" => {
                let path = parts.next().map(PathBuf::from).unwrap_or_else(|| {
                    std::env::current_dir()
                        .unwrap_or_else(|_| PathBuf::from("."))
                        .join("run_log.txt")
                });
                self.log_path = Some(path.clone());
                self.log_input(line);
                println!("\x1b[2m[logging to {}]\x1b[0m", path.display());
                return Ok(false);
            }
            "logstop" => {
                if self.log_path.take().is_some() {
                    println!("\x1b[2m[logging stopped]\x1b[0m");
                } else {
                    println!("\x1b[2m(not logging)\x1b[0m");
                }
                return Ok(false);
            }
            "logstate" => {
                if let Some(ref p) = self.log_path {
                    println!("\x1b[2mlogging: {}\x1b[0m", p.display());
                } else {
                    println!("\x1b[2m(not logging)\x1b[0m");
                }
                return Ok(false);
            }
            "macro" => {
                let sub = parts.next();
                if sub == Some("run") {
                    if let Some(name) = parts.next() {
                        if let Some(code) = self.macros.get(name) {
                            self.execute_payload(ExecutionPayload::Inline {
                                code: code.clone(),
                                args: Vec::new(),
                            })?;
                        } else {
                            println!("\x1b[31m[run]\x1b[0m unknown macro: {name}");
                        }
                    } else {
                        println!("usage: :macro run <NAME>");
                    }
                } else if let Some(name) = sub {
                    let len = self.history_entries.len();
                    let mut indices: Vec<usize> = Vec::new();
                    for part in parts {
                        let (s, e) = parse_history_range(part, len);
                        for i in s..e {
                            indices.push(i);
                        }
                    }
                    indices.sort_unstable();
                    indices.dedup();
                    let code: String = indices
                        .into_iter()
                        .filter_map(|i| self.history_entries.get(i))
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("\n");
                    if code.is_empty() {
                        println!("\x1b[31m[run]\x1b[0m no history entries for range");
                    } else {
                        self.macros.insert(name.to_string(), code);
                        println!("\x1b[2m[macro '{name}' saved]\x1b[0m");
                    }
                } else {
                    println!("usage: :macro <NAME> <range>...  or  :macro run <NAME>");
                }
                return Ok(false);
            }
            "time" => {
                let code = parts.collect::<Vec<_>>().join(" ").trim().to_string();
                if code.is_empty() {
                    println!("usage: :time <CODE>");
                    return Ok(false);
                }
                let start = Instant::now();
                self.execute_payload(ExecutionPayload::Inline {
                    code,
                    args: Vec::new(),
                })?;
                let elapsed = start.elapsed();
                println!("\x1b[2m[elapsed: {:?}]\x1b[0m", elapsed);
                return Ok(false);
            }
            "who" => {
                let mut names: Vec<_> = self.defined_names.iter().cloned().collect();
                names.sort();
                if names.is_empty() {
                    println!("\x1b[2m(no names tracked)\x1b[0m");
                } else {
                    for n in &names {
                        println!("  {n}");
                    }
                }
                return Ok(false);
            }
            "whos" => {
                let pattern = parts.next();
                let mut names: Vec<_> = self.defined_names.iter().cloned().collect();
                if let Some(pat) = pattern {
                    names.retain(|n| n.contains(pat));
                }
                names.sort();
                if names.is_empty() {
                    println!("\x1b[2m(no names tracked)\x1b[0m");
                } else {
                    for n in &names {
                        println!("  {n}");
                    }
                }
                return Ok(false);
            }
            "xmode" => {
                match parts.next().map(|s| s.to_lowercase()) {
                    Some(ref m) if m == "plain" => self.xmode = XMode::Plain,
                    Some(ref m) if m == "context" => self.xmode = XMode::Context,
                    Some(ref m) if m == "verbose" => self.xmode = XMode::Verbose,
                    _ => {
                        let current = match self.xmode {
                            XMode::Plain => "plain",
                            XMode::Context => "context",
                            XMode::Verbose => "verbose",
                        };
                        println!(
                            "\x1b[2mexception display: {current} (plain | context | verbose)\x1b[0m"
                        );
                    }
                }
                return Ok(false);
            }
            "paste" => {
                self.paste_buffer = Some(Vec::new());
                println!("\x1b[2m[paste mode — type :end or Ctrl-D to execute]\x1b[0m");
                return Ok(false);
            }
            "end" => {
                if self.paste_buffer.is_some() {
                    // Handled in main loop (needs editor); show message if somehow we get here
                    println!("\x1b[2m[paste done]\x1b[0m");
                } else {
                    println!("\x1b[31m[run]\x1b[0m not in paste mode");
                }
                return Ok(false);
            }
            "precision" => {
                match parts.next() {
                    None => match self.precision {
                        Some(n) => println!("\x1b[2mprecision: {n}\x1b[0m"),
                        None => println!("\x1b[2mprecision: (default)\x1b[0m"),
                    },
                    Some(s) => {
                        if let Ok(n) = s.parse::<u32>() {
                            let n = n.min(32);
                            self.precision = Some(n);
                            let mut cfg = load_repl_config().unwrap_or_default();
                            cfg.insert("precision".to_string(), n.to_string());
                            if save_repl_config(&cfg).is_err() {
                                println!("\x1b[31m[run]\x1b[0m failed to save config");
                            } else {
                                println!("\x1b[2m[precision = {n}]\x1b[0m");
                            }
                        } else {
                            println!("\x1b[31m[run]\x1b[0m precision must be a number (0–32)");
                        }
                    }
                }
                return Ok(false);
            }
            "config" => {
                let key = parts.next().map(|s| s.to_lowercase());
                let val = parts.next();
                match (key.as_deref(), val) {
                    (None, _) => {
                        let detect = if self.detect_enabled { "on" } else { "off" };
                        let xmode = match self.xmode {
                            XMode::Plain => "plain",
                            XMode::Context => "context",
                            XMode::Verbose => "verbose",
                        };
                        let precision = self
                            .precision
                            .map(|n| n.to_string())
                            .unwrap_or_else(|| "default".to_string());
                        let numbered = if self.numbered_prompts { "on" } else { "off" };
                        println!("\x1b[2mdetect\t{detect}\x1b[0m");
                        println!("\x1b[2mxmode\t{xmode}\x1b[0m");
                        println!("\x1b[2mprecision\t{precision}\x1b[0m");
                        println!("\x1b[2mnumbered_prompts\t{numbered}\x1b[0m");
                    }
                    (Some(k), None) => {
                        let v: Option<String> = match k {
                            "detect" => {
                                Some(if self.detect_enabled { "on" } else { "off" }.to_string())
                            }
                            "xmode" => Some(
                                match self.xmode {
                                    XMode::Plain => "plain",
                                    XMode::Context => "context",
                                    XMode::Verbose => "verbose",
                                }
                                .to_string(),
                            ),
                            "precision" => Some(
                                self.precision
                                    .map(|n| n.to_string())
                                    .unwrap_or_else(|| "default".to_string()),
                            ),
                            "numbered_prompts" => {
                                Some(if self.numbered_prompts { "on" } else { "off" }.to_string())
                            }
                            _ => None,
                        };
                        if let Some(v) = v {
                            println!("{v}");
                        } else {
                            println!("\x1b[31m[run]\x1b[0m unknown config key: {k}");
                        }
                    }
                    (Some(k), Some(v)) => {
                        let mut cfg = load_repl_config().unwrap_or_default();
                        match k {
                            "detect" => {
                                self.detect_enabled =
                                    matches!(v.to_lowercase().as_str(), "on" | "true" | "1");
                                cfg.insert("detect".to_string(), v.to_string());
                            }
                            "xmode" => {
                                self.xmode = match v.to_lowercase().as_str() {
                                    "plain" => XMode::Plain,
                                    "context" => XMode::Context,
                                    _ => XMode::Verbose,
                                };
                                cfg.insert("xmode".to_string(), v.to_string());
                            }
                            "precision" => {
                                if let Ok(n) = v.parse::<u32>() {
                                    self.precision = Some(n.min(32));
                                    cfg.insert(
                                        "precision".to_string(),
                                        self.precision.unwrap().to_string(),
                                    );
                                }
                            }
                            "numbered_prompts" => {
                                self.numbered_prompts =
                                    matches!(v.to_lowercase().as_str(), "on" | "true" | "1");
                                cfg.insert("numbered_prompts".to_string(), v.to_string());
                            }
                            _ => {
                                println!("\x1b[31m[run]\x1b[0m unknown config key: {k}");
                                return Ok(false);
                            }
                        }
                        if save_repl_config(&cfg).is_err() {
                            println!("\x1b[31m[run]\x1b[0m failed to save config");
                        } else {
                            println!("\x1b[2m[{k} = {}]\x1b[0m", v.trim());
                        }
                    }
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
                let rest: Vec<&str> = parts.collect();
                let mut grep_pattern: Option<&str> = None;
                let mut out_file: Option<&str> = None;
                let mut unique = false;
                let mut range_or_limit: Option<String> = None;
                let mut i = 0;
                while i < rest.len() {
                    match rest[i] {
                        "-g" => {
                            i += 1;
                            if i < rest.len() {
                                grep_pattern = Some(rest[i]);
                                i += 1;
                            } else {
                                println!("usage: :history -g <pattern>");
                                return Ok(false);
                            }
                        }
                        "-f" => {
                            i += 1;
                            if i < rest.len() {
                                out_file = Some(rest[i]);
                                i += 1;
                            } else {
                                println!("usage: :history -f <file>");
                                return Ok(false);
                            }
                        }
                        "-u" => {
                            unique = true;
                            i += 1;
                        }
                        _ => {
                            range_or_limit = Some(rest[i].to_string());
                            i += 1;
                        }
                    }
                }
                let entries = &self.history_entries;
                let len = entries.len();
                let (start, end) = if let Some(ref r) = range_or_limit {
                    parse_history_range(r, len)
                } else {
                    (len.saturating_sub(25), len)
                };
                let end = end.min(len);
                let slice = if start < len && start < end {
                    &entries[start..end]
                } else {
                    &entries[0..0]
                };
                let mut selected: Vec<(usize, &String)> = slice
                    .iter()
                    .enumerate()
                    .map(|(i, e)| (start + i + 1, e))
                    .filter(|(_, e)| grep_pattern.map(|p| e.contains(p)).unwrap_or(true))
                    .collect();
                if unique {
                    let mut seen = HashSet::new();
                    selected.retain(|(_, e)| seen.insert((*e).clone()));
                }
                if let Some(path) = out_file {
                    let path = Path::new(path);
                    if let Ok(mut f) = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(path)
                    {
                        use std::io::Write;
                        for (_, e) in &selected {
                            let _ = writeln!(f, "{e}");
                        }
                        println!(
                            "\x1b[2m[appended {} entries to {}]\x1b[0m",
                            selected.len(),
                            path.display()
                        );
                    } else {
                        println!(
                            "\x1b[31m[run]\x1b[0m could not open {} for writing",
                            path.display()
                        );
                    }
                } else if selected.is_empty() {
                    println!("\x1b[2m(no history)\x1b[0m");
                } else {
                    for (num, entry) in selected {
                        let first_line = entry.lines().next().unwrap_or(entry.as_str());
                        let is_multiline = entry.contains('\n');
                        if is_multiline {
                            println!("\x1b[2m[{num:>4}]\x1b[0m {first_line} \x1b[2m(...)\x1b[0m");
                        } else {
                            println!("\x1b[2m[{num:>4}]\x1b[0m {entry}");
                        }
                    }
                }
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
            args: Vec::new(),
        };
        self.execute_payload(payload)
    }

    fn session_var_names(&self) -> Vec<String> {
        self.defined_names.iter().cloned().collect()
    }

    fn execute_payload(&mut self, payload: ExecutionPayload) -> Result<()> {
        let language = self.current_language.clone();
        let outcome = match payload {
            ExecutionPayload::Inline { code, .. } => {
                if self.engine_supports_sessions(&language)? {
                    self.eval_in_session(&language, &code)?
                } else {
                    let engine = self
                        .registry
                        .resolve(&language)
                        .context("language engine not found")?;
                    engine.execute(&ExecutionPayload::Inline {
                        code,
                        args: Vec::new(),
                    })?
                }
            }
            ExecutionPayload::File { ref path, .. } => {
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
            ExecutionPayload::Stdin { code, .. } => {
                if self.engine_supports_sessions(&language)? {
                    self.eval_in_session(&language, &code)?
                } else {
                    let engine = self
                        .registry
                        .resolve(&language)
                        .context("language engine not found")?;
                    engine.execute(&ExecutionPayload::Stdin {
                        code,
                        args: Vec::new(),
                    })?
                }
            }
        };
        render_outcome(&outcome, self.xmode);
        self.last_stdout = Some(outcome.stdout.clone());
        self.in_count += 1;
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

    /// Run introspection (:? EXPR). Python: help(expr) in session. Others: not available.
    fn run_introspect(&mut self, expr: &str) -> Result<()> {
        let language = self.current_language.clone();
        let lang = language.canonical_id();
        if lang == "python" {
            let code = format!("help({expr})");
            let outcome = self.eval_in_session(&language, &code)?;
            render_outcome(&outcome, self.xmode);
            Ok(())
        } else {
            println!(
                "\x1b[2mIntrospection not available for {lang}. Use :? in a Python session.\x1b[0m"
            );
            Ok(())
        }
    }

    /// Run :debug [CODE]. Python: pdb on temp file. Others: not available.
    fn run_debug(&self, code: &str) -> Result<()> {
        let lang = self.current_language.canonical_id();
        if lang != "python" {
            println!(
                "\x1b[2mDebug not available for {lang}. Use :debug in a Python session (pdb).\x1b[0m"
            );
            return Ok(());
        }
        let mut tmp = tempfile::NamedTempFile::new().context("create temp file for :debug")?;
        tmp.as_file_mut()
            .write_all(code.as_bytes())
            .context("write debug script")?;
        let path = tmp.path();
        let status = Command::new("python3")
            .args(["-m", "pdb", path.to_str().unwrap_or("")])
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .context("run pdb")?;
        if !status.success() {
            println!("\x1b[2m[pdb exit {status}]\x1b[0m");
        }
        Ok(())
    }

    fn print_languages(&self) {
        let mut languages = self.registry.known_languages();
        languages.sort();
        println!("available languages: {}", languages.join(", "));
    }

    fn print_versions(&self, language: Option<LanguageSpec>) -> Result<()> {
        println!("language toolchain versions...\n");

        let mut available = 0u32;
        let mut missing = 0u32;

        let mut languages: Vec<String> = if let Some(lang) = language {
            vec![lang.canonical_id().to_string()]
        } else {
            self.registry
                .known_languages()
                .into_iter()
                .map(|value| value.to_string())
                .collect()
        };
        languages.sort();

        for lang_id in &languages {
            let spec = LanguageSpec::new(lang_id.to_string());
            if let Some(engine) = self.registry.resolve(&spec) {
                match engine.toolchain_version() {
                    Ok(Some(version)) => {
                        available += 1;
                        println!(
                            "  [\x1b[32m OK \x1b[0m] {:<14} {} - {}",
                            engine.display_name(),
                            lang_id,
                            version
                        );
                    }
                    Ok(None) => {
                        available += 1;
                        println!(
                            "  [\x1b[33m ?? \x1b[0m] {:<14} {} - unknown",
                            engine.display_name(),
                            lang_id
                        );
                    }
                    Err(_) => {
                        missing += 1;
                        println!(
                            "  [\x1b[31mMISS\x1b[0m] {:<14} {}",
                            engine.display_name(),
                            lang_id
                        );
                    }
                }
            }
        }

        println!();
        println!(
            "  {} available, {} missing, {} total",
            available,
            missing,
            available + missing
        );

        if missing > 0 {
            println!("\n  Tip: Install missing toolchains to enable those languages.");
        }

        Ok(())
    }

    fn install_package(&self, package: &str) {
        let lang_id = self.current_language.canonical_id();
        let override_key = format!("RUN_INSTALL_COMMAND_{}", lang_id.to_ascii_uppercase());
        let override_value = std::env::var(&override_key).ok();
        let Some(mut cmd) = build_install_command(lang_id, package) else {
            if override_value.is_some() {
                println!(
                    "Error: {override_key} is set but could not be parsed.\n\
                     Provide a valid command, e.g. {override_key}=\"uv pip install {{package}}\""
                );
                return;
            }
            println!(
                "No package manager available for '{lang_id}'.\n\
                 Tip: set {override_key}=\"<cmd> {{package}}\"",
            );
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
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let program = cmd.get_program().to_string_lossy();
                println!("\x1b[31m[run]\x1b[0m Package manager not found: {program}");
                println!("Tip: install it or set {override_key}=\"<cmd> {{package}}\"");
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

    fn log_input(&self, line: &str) {
        if let Some(ref p) = self.log_path {
            let _ = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(p)
                .and_then(|mut f| writeln!(f, "{line}"));
        }
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

    fn print_help(&self) {
        println!("\x1b[1mCommands\x1b[0m");
        for (name, desc) in CMD_HELP {
            println!("  \x1b[36m:{name}\x1b[0m  \x1b[2m{desc}\x1b[0m");
        }
        println!("\x1b[2mLanguage shortcuts: :py, :js, :rs, :go, :cpp, :java, ...\x1b[0m");
        println!(
            "\x1b[2mIn session languages (e.g. Python), _ is the last expression result.\x1b[0m"
        );
    }

    fn print_cmd_help(cmd_name: &str) {
        let key = cmd_name.trim_start_matches(':').trim().to_lowercase();
        for (name, desc) in CMD_HELP {
            if name.to_lowercase() == key {
                println!("  \x1b[36m:{name}\x1b[0m  \x1b[2m{desc}\x1b[0m");
                return;
            }
        }
        println!("\x1b[31m[run]\x1b[0m unknown command :{cmd_name}");
    }

    fn print_commands_machine() {
        for (name, desc) in CMD_HELP {
            println!(":{name}\t{desc}");
        }
    }

    fn print_quickref() {
        println!("\x1b[1mQuick reference\x1b[0m");
        for (name, desc) in CMD_HELP {
            println!("  :{name}\t{desc}");
        }
        println!(
            "\x1b[2m:py :js :rs :go :cpp :java ...  In Python (session), _ = last result.\x1b[0m"
        );
    }

    fn shutdown(&mut self) {
        for (_, mut session) in self.sessions.drain() {
            let _ = session.shutdown();
        }
    }
}

/// Strip REPL prompts (>>> and ...) from pasted lines and dedent.
fn strip_paste_prompts(lines: &[String]) -> String {
    let stripped: Vec<String> = lines
        .iter()
        .map(|s| {
            let t = s.trim_start();
            let t = t.strip_prefix(">>>").map(|r| r.trim_start()).unwrap_or(t);
            let t = t.strip_prefix("...").map(|r| r.trim_start()).unwrap_or(t);
            t.to_string()
        })
        .collect();
    let non_empty: Vec<&str> = stripped
        .iter()
        .map(String::as_str)
        .filter(|s| !s.is_empty())
        .collect();
    if non_empty.is_empty() {
        return stripped.join("\n");
    }
    let min_indent = non_empty
        .iter()
        .map(|s| s.len() - s.trim_start().len())
        .min()
        .unwrap_or(0);
    let out: Vec<String> = stripped
        .iter()
        .map(|s| {
            if s.is_empty() {
                s.clone()
            } else {
                let n = (s.len() - s.trim_start().len()).min(min_indent);
                s[n..].to_string()
            }
        })
        .collect();
    out.join("\n")
}

fn apply_xmode(stderr: &str, xmode: XMode) -> String {
    let lines: Vec<&str> = stderr.lines().collect();
    match xmode {
        XMode::Plain => lines.first().map(|s| (*s).to_string()).unwrap_or_default(),
        XMode::Context => lines.iter().take(5).cloned().collect::<Vec<_>>().join("\n"),
        XMode::Verbose => stderr.to_string(),
    }
}

/// Render execution outcome. Stdout is passed through unchanged so engine ANSI/rich output is preserved.
fn render_outcome(outcome: &ExecutionOutcome, xmode: XMode) {
    if !outcome.stdout.is_empty() {
        print!("{}", ensure_trailing_newline(&outcome.stdout));
    }
    if !outcome.stderr.is_empty() {
        let formatted =
            output::format_stderr(&outcome.language, &outcome.stderr, outcome.success());
        let trimmed = apply_xmode(&formatted, xmode);
        if !trimmed.is_empty() {
            eprint!("\x1b[31m{}\x1b[0m", ensure_trailing_newline(&trimmed));
        }
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

/// Run $EDITOR (or vi/notepad) on the given path. Blocks until editor exits.
fn run_editor(path: &Path) -> Result<()> {
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| {
        #[cfg(unix)]
        let default = "vi";
        #[cfg(windows)]
        let default = "notepad";
        default.to_string()
    });
    let path_str = path.to_string_lossy();
    let status = Command::new(&editor)
        .arg(path_str.as_ref())
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .context("run editor")?;
    if !status.success() {
        bail!("editor exited with {}", status);
    }
    Ok(())
}

/// Create a temp file, run the editor on it, return the path (temp file is kept).
fn edit_temp_file() -> Result<PathBuf> {
    let tmp = tempfile::NamedTempFile::new().context("create temp file for :edit")?;
    let path = tmp.path().to_path_buf();
    run_editor(&path)?;
    std::mem::forget(tmp);
    Ok(path)
}

/// Fetch URL to a temp file and return its path. Caller runs and then temp file is left in /tmp.
fn fetch_url_to_temp(url: &str) -> Result<PathBuf> {
    let tmp = tempfile::NamedTempFile::new().context("create temp file for :load url")?;
    let path = tmp.path().to_path_buf();

    #[cfg(unix)]
    let ok = Command::new("curl")
        .args(["-sSL", "-o", path.to_str().unwrap_or(""), url])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    #[cfg(windows)]
    let ok = Command::new("curl")
        .args(["-sSL", "-o", path.to_str().unwrap_or(""), url])
        .status()
        .map(|s| s.success())
        .unwrap_or_else(|_| {
            // Fallback: PowerShell
            let ps = format!(
                "Invoke-WebRequest -Uri '{}' -OutFile '{}' -UseBasicParsing",
                url.replace('\'', "''"),
                path.to_string_lossy().replace('\'', "''")
            );
            Command::new("powershell")
                .args(["-NoProfile", "-Command", &ps])
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        });

    if !ok {
        let _ = tmp.close();
        bail!("fetch failed (curl or download failed)");
    }
    std::mem::forget(tmp);
    Ok(path)
}

/// Run a shell command. If `capture` is true, run and print stdout/stderr; otherwise inherit.
fn run_shell(cmd: &str, capture: bool) {
    #[cfg(unix)]
    let mut c = Command::new("sh");
    #[cfg(unix)]
    c.arg("-c").arg(cmd);

    #[cfg(windows)]
    let mut c = {
        let shell = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string());
        let mut com = Command::new(shell);
        com.arg("/c").arg(cmd);
        com
    };

    if capture {
        match c
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
        {
            Ok(out) => {
                let _ = std::io::stdout().write_all(&out.stdout);
                let _ = std::io::stderr().write_all(&out.stderr);
                if let Some(code) = out.status.code()
                    && code != 0
                {
                    println!("\x1b[2m[exit {code}]\x1b[0m");
                }
            }
            Err(e) => {
                eprintln!("\x1b[31m[run]\x1b[0m shell: {e}");
            }
        }
    } else {
        c.stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit());
        if let Err(e) = c.status() {
            eprintln!("\x1b[31m[run]\x1b[0m shell: {e}");
        }
    }
}

/// Parse :history range: "4-6", "4-", "-6", or "10" (last n). 1-based; returns (start, end) 0-based for entries[start..end].
fn parse_history_range(s: &str, len: usize) -> (usize, usize) {
    if s.contains('-') {
        let (a, b) = s.split_once('-').unwrap_or((s, ""));
        let a = a.trim();
        let b = b.trim();
        if a.is_empty() && b.is_empty() {
            return (len.saturating_sub(25), len);
        }
        if b.is_empty() {
            // "4-" = from 4 to end
            if let Ok(n) = a.parse::<usize>() {
                let start = n.saturating_sub(1).min(len);
                return (start, len);
            }
        } else if a.is_empty() {
            // "-6" = last 6
            if let Ok(n) = b.parse::<usize>() {
                let start = len.saturating_sub(n);
                return (start, len);
            }
        } else if let (Ok(lo), Ok(hi)) = (a.parse::<usize>(), b.parse::<usize>()) {
            // "4-6" = 4 to 6 inclusive
            let start = lo.saturating_sub(1).min(len);
            let end = hi.min(len).max(start);
            return (start, end);
        }
    } else if let Ok(n) = s.parse::<usize>() {
        // "10" = last 10
        let start = len.saturating_sub(n);
        return (start, len);
    }
    (len.saturating_sub(25), len)
}

fn history_path() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("HOME") {
        return Some(Path::new(&home).join(HISTORY_FILE));
    }
    #[cfg(windows)]
    if let Ok(home) = std::env::var("USERPROFILE") {
        return Some(Path::new(&home).join(HISTORY_FILE));
    }
    None
}

fn bookmarks_path() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("HOME") {
        return Some(Path::new(&home).join(BOOKMARKS_FILE));
    }
    #[cfg(windows)]
    if let Ok(home) = std::env::var("USERPROFILE") {
        return Some(Path::new(&home).join(BOOKMARKS_FILE));
    }
    None
}

fn load_bookmarks() -> Result<HashMap<String, PathBuf>> {
    let path = bookmarks_path().context("no home dir for bookmarks")?;
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Ok(HashMap::new()),
    };
    let mut out = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((name, rest)) = line.split_once('\t') {
            let name = name.trim().to_string();
            let p = PathBuf::from(rest.trim());
            if !name.is_empty() && p.is_absolute() {
                out.insert(name, p);
            }
        }
    }
    Ok(out)
}

fn save_bookmarks(bookmarks: &HashMap<String, PathBuf>) -> Result<()> {
    let path = bookmarks_path().context("no home dir for bookmarks")?;
    let mut lines: Vec<String> = bookmarks
        .iter()
        .map(|(k, v)| format!("{}\t{}", k, v.display()))
        .collect();
    lines.sort();
    std::fs::write(path, lines.join("\n") + "\n")?;
    Ok(())
}

fn repl_config_path() -> Option<PathBuf> {
    #[cfg(unix)]
    if let Ok(home) = std::env::var("HOME") {
        return Some(PathBuf::from(&home).join(REPL_CONFIG_FILE));
    }
    #[cfg(windows)]
    if let Ok(home) = std::env::var("USERPROFILE") {
        return Some(PathBuf::from(&home).join(REPL_CONFIG_FILE));
    }
    None
}

fn load_repl_config() -> Result<HashMap<String, String>> {
    let path = repl_config_path().context("no home dir for REPL config")?;
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Ok(HashMap::new()),
    };
    let mut out = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim().to_lowercase();
            let v = v.trim().trim_matches('"').to_string();
            if !k.is_empty() {
                out.insert(k, v);
            }
        }
    }
    Ok(out)
}

fn save_repl_config(cfg: &HashMap<String, String>) -> Result<()> {
    let path = repl_config_path().context("no home dir for REPL config")?;
    let mut keys: Vec<_> = cfg.keys().collect();
    keys.sort();
    let lines: Vec<String> = keys
        .iter()
        .map(|k| {
            let v = cfg.get(*k).unwrap();
            format!("{}={}", k, v)
        })
        .collect();
    std::fs::write(path, lines.join("\n") + "\n")?;
    Ok(())
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
    fn python_auto_indents_nested_blocks() {
        let mut p = PendingInput::new();
        p.push_line("def bubble_sort(arr):");
        p.push_line_auto("python", "n = len(arr)");
        p.push_line_auto("python", "for i in range(n):");
        p.push_line_auto("python", "for j in range(0, n-i-1):");
        p.push_line_auto("python", "if arr[j] > arr[j+1]:");
        p.push_line_auto("python", "arr[j], arr[j+1] = arr[j+1], arr[j]");
        let code = p.take();
        assert!(
            code.contains("    n = len(arr)\n"),
            "missing indent for len"
        );
        assert!(
            code.contains("    for i in range(n):\n"),
            "missing indent for outer loop"
        );
        assert!(
            code.contains("        for j in range(0, n-i-1):\n"),
            "missing indent for inner loop"
        );
        assert!(
            code.contains("            if arr[j] > arr[j+1]:\n"),
            "missing indent for if"
        );
        assert!(
            code.contains("                arr[j], arr[j+1] = arr[j+1], arr[j]\n"),
            "missing indent for swap"
        );
    }

    #[test]
    fn python_auto_dedents_else_like_blocks() {
        let mut p = PendingInput::new();
        p.push_line("def f():");
        p.push_line_auto("python", "if True:");
        p.push_line_auto("python", "print('yes')");
        p.push_line_auto("python", "else:");
        let code = p.take();
        assert!(
            code.contains("    else:\n"),
            "expected else to align with if block:\n{code}"
        );
    }

    #[test]
    fn python_auto_dedents_return_to_def() {
        let mut p = PendingInput::new();
        p.push_line("def bubble_sort(arr):");
        p.push_line_auto("python", "for i in range(3):");
        p.push_line_auto("python", "for j in range(2):");
        p.push_line_auto("python", "if j:");
        p.push_line_auto("python", "pass");
        p.push_line_auto("python", "return arr");
        let code = p.take();
        assert!(
            code.contains("    return arr\n"),
            "expected return to align with def block:\n{code}"
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

    #[test]
    fn generic_multiline_accepts_preprocessor_lines() {
        let mut p = PendingInput::new();
        p.push_line("#include <stdio.h>");
        assert!(
            !p.needs_more_input("c"),
            "preprocessor lines should not force continuation"
        );
    }
}
