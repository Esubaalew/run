use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use anyhow::{Context, Result};
use tempfile::{Builder, TempDir};

use super::{
    ExecutionOutcome, ExecutionPayload, LanguageEngine, LanguageSession, cache_store, hash_source,
    try_cached_execution,
};

pub struct CEngine {
    compiler: Option<PathBuf>,
}

impl Default for CEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl CEngine {
    pub fn new() -> Self {
        Self {
            compiler: resolve_c_compiler(),
        }
    }

    fn ensure_compiler(&self) -> Result<&Path> {
        self.compiler.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "C support requires a C compiler such as `cc`, `clang`, or `gcc`. Install one and ensure it is on your PATH."
            )
        })
    }

    fn write_source(&self, code: &str, dir: &Path) -> Result<PathBuf> {
        let source_path = dir.join("main.c");
        let prepared = prepare_inline_source(code);
        std::fs::write(&source_path, prepared).with_context(|| {
            format!(
                "failed to write temporary C source to {}",
                source_path.display()
            )
        })?;
        Ok(source_path)
    }

    fn copy_source(&self, original: &Path, dir: &Path) -> Result<PathBuf> {
        let target = dir.join("main.c");
        std::fs::copy(original, &target).with_context(|| {
            format!(
                "failed to copy C source from {} to {}",
                original.display(),
                target.display()
            )
        })?;
        Ok(target)
    }

    fn compile(&self, source: &Path, output: &Path) -> Result<std::process::Output> {
        let compiler = self.ensure_compiler()?;
        let mut cmd = Command::new(compiler);
        cmd.arg(source)
            .arg("-std=c11")
            .arg("-O0")
            .arg("-Wall")
            .arg("-Wextra")
            .arg("-o")
            .arg(output)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd.output().with_context(|| {
            format!(
                "failed to invoke {} to compile {}",
                compiler.display(),
                source.display()
            )
        })
    }

    fn run_binary(&self, binary: &Path) -> Result<std::process::Output> {
        let mut cmd = Command::new(binary);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        cmd.stdin(Stdio::inherit());
        cmd.output()
            .with_context(|| format!("failed to execute compiled binary {}", binary.display()))
    }

    fn binary_path(dir: &Path) -> PathBuf {
        let mut path = dir.join("run_c_binary");
        let suffix = std::env::consts::EXE_SUFFIX;
        if !suffix.is_empty() {
            if let Some(stripped) = suffix.strip_prefix('.') {
                path.set_extension(stripped);
            } else {
                path = PathBuf::from(format!("{}{}", path.display(), suffix));
            }
        }
        path
    }
}

impl LanguageEngine for CEngine {
    fn id(&self) -> &'static str {
        "c"
    }

    fn display_name(&self) -> &'static str {
        "C"
    }

    fn aliases(&self) -> &[&'static str] {
        &["ansi-c"]
    }

    fn supports_sessions(&self) -> bool {
        true
    }

    fn validate(&self) -> Result<()> {
        let compiler = self.ensure_compiler()?;
        let mut cmd = Command::new(compiler);
        cmd.arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        cmd.status()
            .with_context(|| format!("failed to invoke {}", compiler.display()))?
            .success()
            .then_some(())
            .ok_or_else(|| anyhow::anyhow!("{} is not executable", compiler.display()))
    }

    fn execute(&self, payload: &ExecutionPayload) -> Result<ExecutionOutcome> {
        // Try cache for inline/stdin payloads
        if let Some(code) = match payload {
            ExecutionPayload::Inline { code } | ExecutionPayload::Stdin { code } => {
                Some(code.as_str())
            }
            _ => None,
        } {
            let prepared = prepare_inline_source(code);
            let src_hash = hash_source(&prepared);
            if let Some(output) = try_cached_execution(src_hash) {
                let start = Instant::now();
                return Ok(ExecutionOutcome {
                    language: self.id().to_string(),
                    exit_code: output.status.code(),
                    stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                    stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                    duration: start.elapsed(),
                });
            }
        }

        let temp_dir = Builder::new()
            .prefix("run-c")
            .tempdir()
            .context("failed to create temporary directory for c build")?;
        let dir_path = temp_dir.path();

        let (source_path, cache_key) = match payload {
            ExecutionPayload::Inline { code } | ExecutionPayload::Stdin { code } => {
                let prepared = prepare_inline_source(code);
                let h = hash_source(&prepared);
                (self.write_source(code, dir_path)?, Some(h))
            }
            ExecutionPayload::File { path } => (self.copy_source(path, dir_path)?, None),
        };

        let binary_path = Self::binary_path(dir_path);
        let start = Instant::now();

        let compile_output = self.compile(&source_path, &binary_path)?;
        if !compile_output.status.success() {
            return Ok(ExecutionOutcome {
                language: self.id().to_string(),
                exit_code: compile_output.status.code(),
                stdout: String::from_utf8_lossy(&compile_output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&compile_output.stderr).into_owned(),
                duration: start.elapsed(),
            });
        }

        if let Some(h) = cache_key {
            cache_store(h, &binary_path);
        }

        let run_output = self.run_binary(&binary_path)?;
        Ok(ExecutionOutcome {
            language: self.id().to_string(),
            exit_code: run_output.status.code(),
            stdout: String::from_utf8_lossy(&run_output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&run_output.stderr).into_owned(),
            duration: start.elapsed(),
        })
    }

    fn start_session(&self) -> Result<Box<dyn LanguageSession>> {
        let compiler = self.ensure_compiler()?.to_path_buf();
        let session = CSession::new(compiler)?;
        Ok(Box::new(session))
    }
}

const SESSION_MAIN_FILE: &str = "main.c";

const PRINT_HELPERS: &str = concat!(
    "static void __print_int(long long value) { printf(\"%lld\\n\", value); }\n",
    "static void __print_uint(unsigned long long value) { printf(\"%llu\\n\", value); }\n",
    "static void __print_double(double value) { printf(\"%0.17g\\n\", value); }\n",
    "static void __print_cstr(const char *value) { if (!value) { printf(\"(null)\\n\"); } else { printf(\"%s\\n\", value); } }\n",
    "static void __print_char(int value) { printf(\"%d\\n\", value); }\n",
    "static void __print_pointer(const void *value) { printf(\"%p\\n\", value); }\n",
    "#define __print(value) _Generic((value), \\\n",
    "    char: __print_char, \\\n",
    "    signed char: __print_int, \\\n",
    "    short: __print_int, \\\n",
    "    int: __print_int, \\\n",
    "    long: __print_int, \\\n",
    "    long long: __print_int, \\\n",
    "    unsigned char: __print_uint, \\\n",
    "    unsigned short: __print_uint, \\\n",
    "    unsigned int: __print_uint, \\\n",
    "    unsigned long: __print_uint, \\\n",
    "    unsigned long long: __print_uint, \\\n",
    "    float: __print_double, \\\n",
    "    double: __print_double, \\\n",
    "    long double: __print_double, \\\n",
    "    char *: __print_cstr, \\\n",
    "    const char *: __print_cstr, \\\n",
    "    default: __print_pointer \\\n",
    ")(value)\n\n",
);

struct CSession {
    compiler: PathBuf,
    workspace: TempDir,
    includes: BTreeSet<String>,
    items: Vec<String>,
    statements: Vec<String>,
    last_stdout: String,
    last_stderr: String,
}

enum CSnippetKind {
    Include(Option<String>),
    Item,
    Statement,
}

impl CSession {
    fn new(compiler: PathBuf) -> Result<Self> {
        let workspace = TempDir::new().context("failed to create C session workspace")?;
        let session = Self {
            compiler,
            workspace,
            includes: Self::default_includes(),
            items: Vec::new(),
            statements: Vec::new(),
            last_stdout: String::new(),
            last_stderr: String::new(),
        };
        session.persist_source()?;
        Ok(session)
    }

    fn default_includes() -> BTreeSet<String> {
        let mut includes = BTreeSet::new();
        includes.insert("#include <stdio.h>".to_string());
        includes.insert("#include <inttypes.h>".to_string());
        includes
    }

    fn language_id(&self) -> &str {
        "c"
    }

    fn source_path(&self) -> PathBuf {
        self.workspace.path().join(SESSION_MAIN_FILE)
    }

    fn binary_path(&self) -> PathBuf {
        CEngine::binary_path(self.workspace.path())
    }

    fn persist_source(&self) -> Result<()> {
        let source = self.render_source();
        fs::write(self.source_path(), source)
            .with_context(|| "failed to write C session source".to_string())
    }

    fn render_source(&self) -> String {
        let mut source = String::new();

        for include in &self.includes {
            source.push_str(include);
            if !include.ends_with('\n') {
                source.push('\n');
            }
        }

        source.push('\n');
        source.push_str(PRINT_HELPERS);

        for item in &self.items {
            source.push_str(item);
            if !item.ends_with('\n') {
                source.push('\n');
            }
            source.push('\n');
        }

        source.push_str("int main(void) {\n");
        if self.statements.is_empty() {
            source.push_str("    // session body\n");
        } else {
            for snippet in &self.statements {
                for line in snippet.lines() {
                    source.push_str("    ");
                    source.push_str(line);
                    source.push('\n');
                }
            }
        }
        source.push_str("    return 0;\n}\n");

        source
    }

    fn compile(&self) -> Result<std::process::Output> {
        let source = self.source_path();
        let binary = self.binary_path();
        self.compile_with_paths(&source, &binary)
    }

    fn compile_with_paths(&self, source: &Path, binary: &Path) -> Result<std::process::Output> {
        let mut cmd = Command::new(&self.compiler);
        cmd.arg(source)
            .arg("-std=c11")
            .arg("-O0")
            .arg("-Wall")
            .arg("-Wextra")
            .arg("-o")
            .arg(binary)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd.output().with_context(|| {
            format!(
                "failed to invoke {} to compile {}",
                self.compiler.display(),
                source.display()
            )
        })
    }

    fn run_binary(&self) -> Result<std::process::Output> {
        let binary = self.binary_path();
        self.run_binary_path(&binary)
    }

    fn run_binary_path(&self, binary: &Path) -> Result<std::process::Output> {
        let mut cmd = Command::new(binary);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        cmd.output()
            .with_context(|| format!("failed to execute compiled binary {}", binary.display()))
    }

    fn run_standalone_program(&self, code: &str) -> Result<ExecutionOutcome> {
        let start = Instant::now();
        let source_path = self.workspace.path().join("standalone.c");
        let binary_path = self.workspace.path().join("standalone_c_binary");

        let mut source = String::new();

        for include in &self.includes {
            source.push_str(include);
            if !include.ends_with('\n') {
                source.push('\n');
            }
        }

        source.push('\n');
        source.push_str(PRINT_HELPERS);

        for item in &self.items {
            source.push_str(item);
            if !item.ends_with('\n') {
                source.push('\n');
            }
            source.push('\n');
        }

        source.push_str(code);
        if !code.ends_with('\n') {
            source.push('\n');
        }

        fs::write(&source_path, source)
            .with_context(|| "failed to write C standalone source".to_string())?;

        let compile_output = self.compile_with_paths(&source_path, &binary_path)?;
        if !compile_output.status.success() {
            return Ok(ExecutionOutcome {
                language: self.language_id().to_string(),
                exit_code: compile_output.status.code(),
                stdout: Self::normalize_output(&compile_output.stdout),
                stderr: Self::normalize_output(&compile_output.stderr),
                duration: start.elapsed(),
            });
        }

        let run_output = self.run_binary_path(&binary_path)?;
        Ok(ExecutionOutcome {
            language: self.language_id().to_string(),
            exit_code: run_output.status.code(),
            stdout: Self::normalize_output(&run_output.stdout),
            stderr: Self::normalize_output(&run_output.stderr),
            duration: start.elapsed(),
        })
    }

    fn add_include(&mut self, line: &str) -> CSnippetKind {
        let added = self.includes.insert(line.to_string());
        if added {
            CSnippetKind::Include(Some(line.to_string()))
        } else {
            CSnippetKind::Include(None)
        }
    }

    fn add_item(&mut self, code: &str) -> CSnippetKind {
        let mut snippet = code.to_string();
        if !snippet.ends_with('\n') {
            snippet.push('\n');
        }
        self.items.push(snippet);
        CSnippetKind::Item
    }

    fn add_statement(&mut self, code: &str) -> CSnippetKind {
        let mut snippet = code.to_string();
        if !snippet.ends_with('\n') {
            snippet.push('\n');
        }
        self.statements.push(snippet);
        CSnippetKind::Statement
    }

    fn add_expression(&mut self, code: &str) -> CSnippetKind {
        let wrapped = wrap_expression(code);
        self.statements.push(wrapped);
        CSnippetKind::Statement
    }

    fn reset_state(&mut self) -> Result<()> {
        self.includes = Self::default_includes();
        self.items.clear();
        self.statements.clear();
        self.last_stdout.clear();
        self.last_stderr.clear();
        self.persist_source()
    }

    fn rollback(&mut self, kind: CSnippetKind) -> Result<()> {
        match kind {
            CSnippetKind::Include(Some(line)) => {
                self.includes.remove(&line);
            }
            CSnippetKind::Include(None) => {}
            CSnippetKind::Item => {
                self.items.pop();
            }
            CSnippetKind::Statement => {
                self.statements.pop();
            }
        }
        self.persist_source()
    }

    fn normalize_output(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes)
            .replace("\r\n", "\n")
            .replace('\r', "")
    }

    fn diff_outputs(previous: &str, current: &str) -> String {
        if let Some(suffix) = current.strip_prefix(previous) {
            suffix.to_string()
        } else {
            current.to_string()
        }
    }

    fn run_insertion(&mut self, kind: CSnippetKind) -> Result<(ExecutionOutcome, bool)> {
        if matches!(kind, CSnippetKind::Include(None)) {
            return Ok((
                ExecutionOutcome {
                    language: self.language_id().to_string(),
                    exit_code: None,
                    stdout: String::new(),
                    stderr: String::new(),
                    duration: Default::default(),
                },
                true,
            ));
        }

        self.persist_source()?;
        let start = Instant::now();
        let compile_output = self.compile()?;

        if !compile_output.status.success() {
            let duration = start.elapsed();
            self.rollback(kind)?;
            let outcome = ExecutionOutcome {
                language: self.language_id().to_string(),
                exit_code: compile_output.status.code(),
                stdout: Self::normalize_output(&compile_output.stdout),
                stderr: Self::normalize_output(&compile_output.stderr),
                duration,
            };
            return Ok((outcome, false));
        }

        let run_output = self.run_binary()?;
        let duration = start.elapsed();
        let stdout_full = Self::normalize_output(&run_output.stdout);
        let stderr_full = Self::normalize_output(&run_output.stderr);

        let stdout = Self::diff_outputs(&self.last_stdout, &stdout_full);
        let stderr = Self::diff_outputs(&self.last_stderr, &stderr_full);

        if run_output.status.success() {
            self.last_stdout = stdout_full;
            self.last_stderr = stderr_full;
            let outcome = ExecutionOutcome {
                language: self.language_id().to_string(),
                exit_code: run_output.status.code(),
                stdout,
                stderr,
                duration,
            };
            return Ok((outcome, true));
        }

        self.rollback(kind)?;
        let outcome = ExecutionOutcome {
            language: self.language_id().to_string(),
            exit_code: run_output.status.code(),
            stdout,
            stderr,
            duration,
        };
        Ok((outcome, false))
    }

    fn run_include(&mut self, line: &str) -> Result<(ExecutionOutcome, bool)> {
        let kind = self.add_include(line);
        self.run_insertion(kind)
    }

    fn run_item(&mut self, code: &str) -> Result<(ExecutionOutcome, bool)> {
        let kind = self.add_item(code);
        self.run_insertion(kind)
    }

    fn run_statement(&mut self, code: &str) -> Result<(ExecutionOutcome, bool)> {
        let kind = self.add_statement(code);
        self.run_insertion(kind)
    }

    fn run_expression(&mut self, code: &str) -> Result<(ExecutionOutcome, bool)> {
        let kind = self.add_expression(code);
        self.run_insertion(kind)
    }
}

impl LanguageSession for CSession {
    fn language_id(&self) -> &str {
        CSession::language_id(self)
    }

    fn eval(&mut self, code: &str) -> Result<ExecutionOutcome> {
        let trimmed = code.trim();
        if trimmed.is_empty() {
            return Ok(ExecutionOutcome {
                language: self.language_id().to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                duration: Instant::now().elapsed(),
            });
        }

        if trimmed.eq_ignore_ascii_case(":reset") {
            self.reset_state()?;
            return Ok(ExecutionOutcome {
                language: self.language_id().to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                duration: Default::default(),
            });
        }

        if trimmed.eq_ignore_ascii_case(":help") {
            return Ok(ExecutionOutcome {
                language: self.language_id().to_string(),
                exit_code: None,
                stdout:
                    "C commands:\n  :reset - clear session state\n  :help  - show this message\n"
                        .to_string(),
                stderr: String::new(),
                duration: Default::default(),
            });
        }

        if contains_main_definition(code) {
            return self.run_standalone_program(code);
        }

        if let Some(include) = parse_include(trimmed) {
            let (outcome, _) = self.run_include(&include)?;
            return Ok(outcome);
        }

        if is_item_snippet(trimmed) {
            let (outcome, _) = self.run_item(code)?;
            return Ok(outcome);
        }

        if should_treat_as_expression(trimmed) {
            let (outcome, success) = self.run_expression(trimmed)?;
            if success {
                return Ok(outcome);
            }
        }

        let (outcome, _) = self.run_statement(code)?;
        Ok(outcome)
    }

    fn shutdown(&mut self) -> Result<()> {
        Ok(())
    }
}

fn contains_main_definition(code: &str) -> bool {
    let bytes = code.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut in_string = false;
    let mut string_delim = b'"';
    let mut in_char = false;

    while i < len {
        let b = bytes[i];

        if in_line_comment {
            if b == b'\n' {
                in_line_comment = false;
            }
            i += 1;
            continue;
        }

        if in_block_comment {
            if b == b'*' && i + 1 < len && bytes[i + 1] == b'/' {
                in_block_comment = false;
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }

        if in_string {
            if b == b'\\' {
                i = (i + 2).min(len);
                continue;
            }
            if b == string_delim {
                in_string = false;
            }
            i += 1;
            continue;
        }

        if in_char {
            if b == b'\\' {
                i = (i + 2).min(len);
                continue;
            }
            if b == b'\'' {
                in_char = false;
            }
            i += 1;
            continue;
        }

        match b {
            b'/' if i + 1 < len && bytes[i + 1] == b'/' => {
                in_line_comment = true;
                i += 2;
                continue;
            }
            b'/' if i + 1 < len && bytes[i + 1] == b'*' => {
                in_block_comment = true;
                i += 2;
                continue;
            }
            b'"' | b'\'' => {
                if b == b'"' {
                    in_string = true;
                    string_delim = b;
                } else {
                    in_char = true;
                }
                i += 1;
                continue;
            }
            b'm' if i + 4 <= len && &bytes[i..i + 4] == b"main" => {
                if i > 0 {
                    let prev = bytes[i - 1];
                    if prev.is_ascii_alphanumeric() || prev == b'_' {
                        i += 1;
                        continue;
                    }
                }

                let after_name = i + 4;
                if after_name < len {
                    let next = bytes[after_name];
                    if next.is_ascii_alphanumeric() || next == b'_' {
                        i += 1;
                        continue;
                    }
                }

                let mut j = after_name;
                while j < len && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j >= len || bytes[j] != b'(' {
                    i += 1;
                    continue;
                }

                let mut depth = 1usize;
                let mut k = j + 1;
                let mut inner_line_comment = false;
                let mut inner_block_comment = false;
                let mut inner_string = false;
                let mut inner_char = false;

                while k < len {
                    let ch = bytes[k];

                    if inner_line_comment {
                        if ch == b'\n' {
                            inner_line_comment = false;
                        }
                        k += 1;
                        continue;
                    }

                    if inner_block_comment {
                        if ch == b'*' && k + 1 < len && bytes[k + 1] == b'/' {
                            inner_block_comment = false;
                            k += 2;
                            continue;
                        }
                        k += 1;
                        continue;
                    }

                    if inner_string {
                        if ch == b'\\' {
                            k = (k + 2).min(len);
                            continue;
                        }
                        if ch == b'"' {
                            inner_string = false;
                        }
                        k += 1;
                        continue;
                    }

                    if inner_char {
                        if ch == b'\\' {
                            k = (k + 2).min(len);
                            continue;
                        }
                        if ch == b'\'' {
                            inner_char = false;
                        }
                        k += 1;
                        continue;
                    }

                    match ch {
                        b'/' if k + 1 < len && bytes[k + 1] == b'/' => {
                            inner_line_comment = true;
                            k += 2;
                            continue;
                        }
                        b'/' if k + 1 < len && bytes[k + 1] == b'*' => {
                            inner_block_comment = true;
                            k += 2;
                            continue;
                        }
                        b'"' => {
                            inner_string = true;
                            k += 1;
                            continue;
                        }
                        b'\'' => {
                            inner_char = true;
                            k += 1;
                            continue;
                        }
                        b'(' => {
                            depth += 1;
                        }
                        b')' => {
                            depth -= 1;
                            k += 1;
                            if depth == 0 {
                                break;
                            } else {
                                continue;
                            }
                        }
                        _ => {}
                    }

                    k += 1;
                }

                if depth != 0 {
                    i += 1;
                    continue;
                }

                let mut after = k;
                loop {
                    while after < len && bytes[after].is_ascii_whitespace() {
                        after += 1;
                    }
                    if after + 1 < len && bytes[after] == b'/' && bytes[after + 1] == b'/' {
                        after += 2;
                        while after < len && bytes[after] != b'\n' {
                            after += 1;
                        }
                        continue;
                    }
                    if after + 1 < len && bytes[after] == b'/' && bytes[after + 1] == b'*' {
                        after += 2;
                        while after + 1 < len {
                            if bytes[after] == b'*' && bytes[after + 1] == b'/' {
                                after += 2;
                                break;
                            }
                            after += 1;
                        }
                        continue;
                    }
                    break;
                }

                if after < len && bytes[after] == b'{' {
                    return true;
                }
            }
            _ => {}
        }

        i += 1;
    }

    false
}

fn parse_include(code: &str) -> Option<String> {
    let trimmed = code.trim_start();
    if !trimmed.starts_with("#include") {
        return None;
    }
    let line = trimmed.lines().next()?.trim().to_string();
    if line.is_empty() { None } else { Some(line) }
}

fn is_item_snippet(code: &str) -> bool {
    let trimmed = code.trim_start();
    if trimmed.starts_with("#include") {
        return false;
    }

    if trimmed.starts_with('#') {
        return true;
    }

    const KEYWORDS: [&str; 8] = [
        "typedef", "struct", "union", "enum", "extern", "static", "const", "volatile",
    ];
    if KEYWORDS.iter().any(|kw| trimmed.starts_with(kw)) {
        return true;
    }

    if let Some(open_brace) = trimmed.find('{') {
        let before_brace = trimmed[..open_brace].trim();
        if before_brace.ends_with(';') {
            return false;
        }

        const CONTROL_KEYWORDS: [&str; 5] = ["if", "for", "while", "switch", "do"];
        if CONTROL_KEYWORDS
            .iter()
            .any(|kw| before_brace.starts_with(kw))
        {
            return false;
        }

        if before_brace.contains('(') {
            return true;
        }
    }

    if trimmed.ends_with(';') {
        if trimmed.contains('(') && trimmed.contains(')') {
            let before_paren = trimmed.split('(').next().unwrap_or_default();
            if before_paren.split_whitespace().count() >= 2 {
                return true;
            }
        }

        let first_token = trimmed.split_whitespace().next().unwrap_or_default();
        const TYPE_PREFIXES: [&str; 12] = [
            "auto", "register", "signed", "unsigned", "short", "long", "int", "char", "float",
            "double", "_Bool", "void",
        ];
        if TYPE_PREFIXES.contains(&first_token) {
            return true;
        }
    }

    false
}

fn should_treat_as_expression(code: &str) -> bool {
    let trimmed = code.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.contains('\n') {
        return false;
    }
    if trimmed.ends_with(';') {
        return false;
    }
    const DISALLOWED_PREFIXES: [&str; 21] = [
        "#", "typedef", "struct", "union", "enum", "extern", "static", "const", "volatile", "void",
        "auto", "signed", "register", "unsigned", "short", "long", "int", "char", "float",
        "double", "_Bool",
    ];
    if DISALLOWED_PREFIXES.iter().any(|kw| trimmed.starts_with(kw)) {
        return false;
    }
    true
}

fn wrap_expression(code: &str) -> String {
    format!("__print({});\n", code)
}

fn prepare_inline_source(code: &str) -> String {
    if !needs_wrapper(code) {
        return code.to_string();
    }

    if code.trim().is_empty() {
        return "#include <stdio.h>\n\nint main(void)\n{\n    return 0;\n}\n".to_string();
    }

    let body = indent_snippet(code);
    format!("#include <stdio.h>\n\nint main(void)\n{{\n{body}    return 0;\n}}\n",)
}

fn needs_wrapper(code: &str) -> bool {
    let trimmed = code.trim();
    if trimmed.is_empty() {
        return true;
    }

    !(trimmed.contains("#include") || trimmed.contains("main("))
}

fn indent_snippet(snippet: &str) -> String {
    let mut result = String::new();
    for line in snippet.lines() {
        if line.trim().is_empty() {
            result.push('\n');
        } else {
            result.push_str("    ");
            result.push_str(line);
            result.push('\n');
        }
    }

    result
}

fn resolve_c_compiler() -> Option<PathBuf> {
    ["cc", "clang", "gcc"]
        .into_iter()
        .find_map(|candidate| which::which(candidate).ok())
}
