use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tempfile::{Builder, TempDir};

use super::{
    ExecutionOutcome, ExecutionPayload, LanguageEngine, LanguageSession, cache_store, hash_source,
    try_cached_execution,
};

pub struct ZigEngine {
    executable: Option<PathBuf>,
}

impl ZigEngine {
    pub fn new() -> Self {
        Self {
            executable: resolve_zig_binary(),
        }
    }

    fn ensure_executable(&self) -> Result<&Path> {
        self.executable.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "Zig support requires the `zig` executable. Install it from https://ziglang.org/download/ and ensure it is on your PATH."
            )
        })
    }

    fn write_temp_source(&self, code: &str) -> Result<(TempDir, PathBuf)> {
        let dir = Builder::new()
            .prefix("run-zig")
            .tempdir()
            .context("failed to create temporary directory for Zig source")?;
        let path = dir.path().join("snippet.zig");
        let mut contents = code.to_string();
        if !contents.ends_with('\n') {
            contents.push('\n');
        }
        std::fs::write(&path, contents).with_context(|| {
            format!("failed to write temporary Zig source to {}", path.display())
        })?;
        Ok((dir, path))
    }

    fn run_source(&self, source: &Path) -> Result<std::process::Output> {
        let executable = self.ensure_executable()?;
        let mut cmd = Command::new(executable);
        cmd.arg("run")
            .arg(source)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd.stdin(Stdio::inherit());
        if let Some(dir) = source.parent() {
            cmd.current_dir(dir);
        }
        cmd.output().with_context(|| {
            format!(
                "failed to execute {} with source {}",
                executable.display(),
                source.display()
            )
        })
    }
}

impl LanguageEngine for ZigEngine {
    fn id(&self) -> &'static str {
        "zig"
    }

    fn display_name(&self) -> &'static str {
        "Zig"
    }

    fn aliases(&self) -> &[&'static str] {
        &["ziglang"]
    }

    fn supports_sessions(&self) -> bool {
        self.executable.is_some()
    }

    fn validate(&self) -> Result<()> {
        let executable = self.ensure_executable()?;
        let mut cmd = Command::new(executable);
        cmd.arg("version")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        cmd.status()
            .with_context(|| format!("failed to invoke {}", executable.display()))?
            .success()
            .then_some(())
            .ok_or_else(|| anyhow::anyhow!("{} is not executable", executable.display()))
    }

    fn execute(&self, payload: &ExecutionPayload) -> Result<ExecutionOutcome> {
        // Try cache for inline/stdin payloads
        if let Some(code) = match payload {
            ExecutionPayload::Inline { code } | ExecutionPayload::Stdin { code } => {
                Some(code.as_str())
            }
            _ => None,
        } {
            let snippet = wrap_inline_snippet(code);
            let src_hash = hash_source(&snippet);
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

        let start = Instant::now();
        let (temp_dir, source_path, cache_key) = match payload {
            ExecutionPayload::Inline { code } | ExecutionPayload::Stdin { code } => {
                let snippet = wrap_inline_snippet(code);
                let h = hash_source(&snippet);
                let (dir, path) = self.write_temp_source(&snippet)?;
                (Some(dir), path, Some(h))
            }
            ExecutionPayload::File { path } => {
                if path.extension().and_then(|e| e.to_str()) != Some("zig") {
                    let code = std::fs::read_to_string(path)?;
                    let (dir, new_path) = self.write_temp_source(&code)?;
                    (Some(dir), new_path, None)
                } else {
                    (None, path.clone(), None)
                }
            }
        };

        // For cacheable code, try zig build-exe + cache
        if let Some(h) = cache_key {
            let executable = self.ensure_executable()?;
            let dir = source_path.parent().unwrap_or(std::path::Path::new("."));
            let bin_path = dir.join("snippet");
            let mut build_cmd = Command::new(executable);
            build_cmd
                .arg("build-exe")
                .arg(&source_path)
                .arg("-femit-bin=snippet")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .current_dir(dir);

            if let Ok(build_output) = build_cmd.output() {
                if build_output.status.success() && bin_path.exists() {
                    cache_store(h, &bin_path);
                    let mut run_cmd = Command::new(&bin_path);
                    run_cmd
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped())
                        .stdin(Stdio::inherit());
                    if let Ok(output) = run_cmd.output() {
                        drop(temp_dir);
                        return Ok(ExecutionOutcome {
                            language: self.id().to_string(),
                            exit_code: output.status.code(),
                            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                            duration: start.elapsed(),
                        });
                    }
                }
            }
        }

        // Fallback to zig run
        let output = self.run_source(&source_path)?;
        drop(temp_dir);

        let mut combined_stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr_str = String::from_utf8_lossy(&output.stderr).into_owned();

        if output.status.success() && !stderr_str.contains("error:") {
            if !combined_stdout.is_empty() && !stderr_str.is_empty() {
                combined_stdout.push_str(&stderr_str);
            } else if combined_stdout.is_empty() {
                combined_stdout = stderr_str.clone();
            }
        }

        Ok(ExecutionOutcome {
            language: self.id().to_string(),
            exit_code: output.status.code(),
            stdout: combined_stdout,
            stderr: if output.status.success() && !stderr_str.contains("error:") {
                String::new()
            } else {
                stderr_str
            },
            duration: start.elapsed(),
        })
    }

    fn start_session(&self) -> Result<Box<dyn LanguageSession>> {
        let executable = self.ensure_executable()?.to_path_buf();
        Ok(Box::new(ZigSession::new(executable)?))
    }
}

fn resolve_zig_binary() -> Option<PathBuf> {
    which::which("zig").ok()
}

const ZIG_NUMERIC_SUFFIXES: [&str; 17] = [
    "usize", "isize", "u128", "i128", "f128", "f80", "u64", "i64", "f64", "u32", "i32", "f32",
    "u16", "i16", "f16", "u8", "i8",
];

fn wrap_inline_snippet(code: &str) -> String {
    let trimmed = code.trim();
    if trimmed.is_empty() || trimmed.contains("pub fn main") {
        let mut owned = code.to_string();
        if !owned.ends_with('\n') {
            owned.push('\n');
        }
        return owned;
    }

    let mut body = String::new();
    for line in code.lines() {
        body.push_str("    ");
        body.push_str(line);
        if !line.ends_with('\n') {
            body.push('\n');
        }
    }
    if body.is_empty() {
        body.push_str("    const stdout = std.io.getStdOut().writer(); _ = stdout.print(\"\\n\", .{}) catch {};\n");
    }

    format!("const std = @import(\"std\");\n\npub fn main() !void {{\n{body}}}\n")
}

struct ZigSession {
    executable: PathBuf,
    workspace: TempDir,
    items: Vec<String>,
    statements: Vec<String>,
    last_stdout: String,
    last_stderr: String,
}

enum ZigSnippetKind {
    Declaration,
    Statement,
    Expression,
}

impl ZigSession {
    fn new(executable: PathBuf) -> Result<Self> {
        let workspace = TempDir::new().context("failed to create Zig session workspace")?;
        let session = Self {
            executable,
            workspace,
            items: Vec::new(),
            statements: Vec::new(),
            last_stdout: String::new(),
            last_stderr: String::new(),
        };
        session.persist_source()?;
        Ok(session)
    }

    fn source_path(&self) -> PathBuf {
        self.workspace.path().join("session.zig")
    }

    fn persist_source(&self) -> Result<()> {
        let source = self.render_source();
        fs::write(self.source_path(), source)
            .with_context(|| "failed to write Zig session source".to_string())
    }

    fn render_source(&self) -> String {
        let mut source = String::from("const std = @import(\"std\");\n\n");

        for item in &self.items {
            source.push_str(item);
            if !item.ends_with('\n') {
                source.push('\n');
            }
            source.push('\n');
        }

        source.push_str("pub fn main() !void {\n");
        if self.statements.is_empty() {
            source.push_str("    return;\n");
        } else {
            for snippet in &self.statements {
                for line in snippet.lines() {
                    source.push_str("    ");
                    source.push_str(line);
                    source.push('\n');
                }
            }
        }
        source.push_str("}\n");

        source
    }

    fn run_program(&self) -> Result<std::process::Output> {
        let mut cmd = Command::new(&self.executable);
        cmd.arg("run")
            .arg("session.zig")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(self.workspace.path());
        cmd.output().with_context(|| {
            format!(
                "failed to execute {} for Zig session",
                self.executable.display()
            )
        })
    }

    fn run_standalone_program(&self, code: &str) -> Result<ExecutionOutcome> {
        let start = Instant::now();
        let path = self.workspace.path().join("standalone.zig");
        let mut contents = code.to_string();
        if !contents.ends_with('\n') {
            contents.push('\n');
        }
        fs::write(&path, contents)
            .with_context(|| "failed to write Zig standalone source".to_string())?;

        let mut cmd = Command::new(&self.executable);
        cmd.arg("run")
            .arg("standalone.zig")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(self.workspace.path());
        let output = cmd.output().with_context(|| {
            format!(
                "failed to execute {} for Zig standalone snippet",
                self.executable.display()
            )
        })?;

        let mut stdout = Self::normalize_output(&output.stdout);
        let stderr = Self::normalize_output(&output.stderr);

        if output.status.success() && !stderr.contains("error:") {
            if stdout.is_empty() {
                stdout = stderr.clone();
            } else {
                stdout.push_str(&stderr);
            }
        }

        Ok(ExecutionOutcome {
            language: self.language_id().to_string(),
            exit_code: output.status.code(),
            stdout,
            stderr: if output.status.success() && !stderr.contains("error:") {
                String::new()
            } else {
                stderr
            },
            duration: start.elapsed(),
        })
    }

    fn run_current(&mut self, start: Instant) -> Result<(ExecutionOutcome, bool)> {
        self.persist_source()?;
        let output = self.run_program()?;
        let mut stdout_full = Self::normalize_output(&output.stdout);
        let stderr_full = Self::normalize_output(&output.stderr);

        let success = output.status.success();

        if success && !stderr_full.is_empty() && !stderr_full.contains("error:") {
            if stdout_full.is_empty() {
                stdout_full = stderr_full.clone();
            } else {
                stdout_full.push_str(&stderr_full);
            }
        }

        let (stdout, stderr) = if success {
            let stdout_delta = Self::diff_outputs(&self.last_stdout, &stdout_full);
            let stderr_clean = if !stderr_full.contains("error:") {
                String::new()
            } else {
                stderr_full.clone()
            };
            let stderr_delta = Self::diff_outputs(&self.last_stderr, &stderr_clean);
            self.last_stdout = stdout_full;
            self.last_stderr = stderr_clean;
            (stdout_delta, stderr_delta)
        } else {
            (stdout_full, stderr_full)
        };

        let outcome = ExecutionOutcome {
            language: self.language_id().to_string(),
            exit_code: output.status.code(),
            stdout,
            stderr,
            duration: start.elapsed(),
        };

        Ok((outcome, success))
    }

    fn apply_declaration(&mut self, code: &str) -> Result<(ExecutionOutcome, bool)> {
        let normalized = normalize_snippet(code);
        let mut snippet = normalized;
        if !snippet.ends_with('\n') {
            snippet.push('\n');
        }
        self.items.push(snippet);
        let start = Instant::now();
        let (outcome, success) = self.run_current(start)?;
        if !success {
            let _ = self.items.pop();
            self.persist_source()?;
        }
        Ok((outcome, success))
    }

    fn apply_statement(&mut self, code: &str) -> Result<(ExecutionOutcome, bool)> {
        let normalized = normalize_snippet(code);
        let snippet = ensure_trailing_newline(&normalized);
        self.statements.push(snippet);
        let start = Instant::now();
        let (outcome, success) = self.run_current(start)?;
        if !success {
            let _ = self.statements.pop();
            self.persist_source()?;
        }
        Ok((outcome, success))
    }

    fn apply_expression(&mut self, code: &str) -> Result<(ExecutionOutcome, bool)> {
        let normalized = normalize_snippet(code);
        let wrapped = wrap_expression(&normalized);
        self.statements.push(wrapped);
        let start = Instant::now();
        let (outcome, success) = self.run_current(start)?;
        if !success {
            let _ = self.statements.pop();
            self.persist_source()?;
        }
        Ok((outcome, success))
    }

    fn reset(&mut self) -> Result<()> {
        self.items.clear();
        self.statements.clear();
        self.last_stdout.clear();
        self.last_stderr.clear();
        self.persist_source()
    }

    fn normalize_output(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes)
            .replace("\r\n", "\n")
            .replace('\r', "")
    }

    fn diff_outputs(previous: &str, current: &str) -> String {
        current
            .strip_prefix(previous)
            .map(|s| s.to_string())
            .unwrap_or_else(|| current.to_string())
    }
}

impl LanguageSession for ZigSession {
    fn language_id(&self) -> &str {
        "zig"
    }

    fn eval(&mut self, code: &str) -> Result<ExecutionOutcome> {
        let trimmed = code.trim();
        if trimmed.is_empty() {
            return Ok(ExecutionOutcome {
                language: self.language_id().to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                duration: Duration::default(),
            });
        }

        if trimmed.eq_ignore_ascii_case(":reset") {
            self.reset()?;
            return Ok(ExecutionOutcome {
                language: self.language_id().to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                duration: Duration::default(),
            });
        }

        if trimmed.eq_ignore_ascii_case(":help") {
            return Ok(ExecutionOutcome {
                language: self.language_id().to_string(),
                exit_code: None,
                stdout:
                    "Zig commands:\n  :reset - clear session state\n  :help  - show this message\n"
                        .to_string(),
                stderr: String::new(),
                duration: Duration::default(),
            });
        }

        if trimmed.contains("pub fn main") {
            return self.run_standalone_program(code);
        }

        match classify_snippet(trimmed) {
            ZigSnippetKind::Declaration => {
                let (outcome, _) = self.apply_declaration(code)?;
                Ok(outcome)
            }
            ZigSnippetKind::Statement => {
                let (outcome, _) = self.apply_statement(code)?;
                Ok(outcome)
            }
            ZigSnippetKind::Expression => {
                let (outcome, _) = self.apply_expression(trimmed)?;
                Ok(outcome)
            }
        }
    }

    fn shutdown(&mut self) -> Result<()> {
        Ok(())
    }
}

fn classify_snippet(code: &str) -> ZigSnippetKind {
    if looks_like_declaration(code) {
        ZigSnippetKind::Declaration
    } else if looks_like_statement(code) {
        ZigSnippetKind::Statement
    } else {
        ZigSnippetKind::Expression
    }
}

fn looks_like_declaration(code: &str) -> bool {
    let trimmed = code.trim_start();
    matches!(
        trimmed,
        t if t.starts_with("const ")
            || t.starts_with("var ")
            || t.starts_with("pub ")
            || t.starts_with("fn ")
            || t.starts_with("usingnamespace ")
            || t.starts_with("extern ")
            || t.starts_with("comptime ")
            || t.starts_with("test ")
    )
}

fn looks_like_statement(code: &str) -> bool {
    let trimmed = code.trim_end();
    trimmed.contains('\n')
        || trimmed.ends_with(';')
        || trimmed.ends_with('}')
        || trimmed.ends_with(':')
        || trimmed.starts_with("//")
        || trimmed.starts_with("/*")
}

fn ensure_trailing_newline(code: &str) -> String {
    let mut snippet = code.to_string();
    if !snippet.ends_with('\n') {
        snippet.push('\n');
    }
    snippet
}

fn wrap_expression(code: &str) -> String {
    format!("std.debug.print(\"{{any}}\\n\", .{{ {} }});", code)
}

fn normalize_snippet(code: &str) -> String {
    rewrite_numeric_suffixes(code)
}

fn rewrite_numeric_suffixes(code: &str) -> String {
    let bytes = code.as_bytes();
    let mut result = String::with_capacity(code.len());
    let mut i = 0;
    while i < bytes.len() {
        let ch = bytes[i] as char;

        if ch == '"' {
            let (segment, advance) = extract_string_literal(&code[i..]);
            result.push_str(segment);
            i += advance;
            continue;
        }

        if ch == '\'' {
            let (segment, advance) = extract_char_literal(&code[i..]);
            result.push_str(segment);
            i += advance;
            continue;
        }

        if ch == '/' && i + 1 < bytes.len() {
            let next = bytes[i + 1] as char;
            if next == '/' {
                result.push_str(&code[i..]);
                break;
            }
            if next == '*' {
                let (segment, advance) = extract_block_comment(&code[i..]);
                result.push_str(segment);
                i += advance;
                continue;
            }
        }

        if ch.is_ascii_digit() {
            if i > 0 {
                let prev = bytes[i - 1] as char;
                if prev.is_ascii_alphanumeric() || prev == '_' {
                    result.push(ch);
                    i += 1;
                    continue;
                }
            }

            let literal_end = scan_numeric_literal(bytes, i);
            if literal_end > i {
                if let Some((suffix, suffix_len)) = match_suffix(&code[literal_end..]) {
                    if !is_identifier_char(bytes, literal_end + suffix_len) {
                        let literal = &code[i..literal_end];
                        result.push_str("@as(");
                        result.push_str(suffix);
                        result.push_str(", ");
                        result.push_str(literal);
                        result.push_str(")");
                        i = literal_end + suffix_len;
                        continue;
                    }
                }

                result.push_str(&code[i..literal_end]);
                i = literal_end;
                continue;
            }
        }

        result.push(ch);
        i += 1;
    }

    if result.len() == code.len() {
        code.to_string()
    } else {
        result
    }
}

fn extract_string_literal(source: &str) -> (&str, usize) {
    let bytes = source.as_bytes();
    let mut i = 1; // skip opening quote
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                i += 2;
            }
            b'"' => {
                i += 1;
                break;
            }
            _ => i += 1,
        }
    }
    (&source[..i], i)
}

fn extract_char_literal(source: &str) -> (&str, usize) {
    let bytes = source.as_bytes();
    let mut i = 1; // skip opening quote
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                i += 2;
            }
            b'\'' => {
                i += 1;
                break;
            }
            _ => i += 1,
        }
    }
    (&source[..i], i)
}

fn extract_block_comment(source: &str) -> (&str, usize) {
    if let Some(idx) = source[2..].find("*/") {
        let end = 2 + idx + 2;
        (&source[..end], end)
    } else {
        (source, source.len())
    }
}

fn scan_numeric_literal(bytes: &[u8], start: usize) -> usize {
    let len = bytes.len();
    if start >= len {
        return start;
    }

    let mut i = start;

    if bytes[i] == b'0' && i + 1 < len {
        match bytes[i + 1] {
            b'x' | b'X' => {
                i += 2;
                while i < len {
                    match bytes[i] {
                        b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F' | b'_' => i += 1,
                        _ => break,
                    }
                }
                return i;
            }
            b'o' | b'O' => {
                i += 2;
                while i < len {
                    match bytes[i] {
                        b'0'..=b'7' | b'_' => i += 1,
                        _ => break,
                    }
                }
                return i;
            }
            b'b' | b'B' => {
                i += 2;
                while i < len {
                    match bytes[i] {
                        b'0' | b'1' | b'_' => i += 1,
                        _ => break,
                    }
                }
                return i;
            }
            _ => {}
        }
    }

    i = start;
    let mut seen_dot = false;
    while i < len {
        match bytes[i] {
            b'0'..=b'9' | b'_' => i += 1,
            b'.' if !seen_dot => {
                if i + 1 < len && bytes[i + 1].is_ascii_digit() {
                    seen_dot = true;
                    i += 1;
                } else {
                    break;
                }
            }
            b'e' | b'E' | b'p' | b'P' => {
                let mut j = i + 1;
                if j < len && (bytes[j] == b'+' || bytes[j] == b'-') {
                    j += 1;
                }
                let mut exp_digits = 0;
                while j < len {
                    match bytes[j] {
                        b'0'..=b'9' | b'_' => {
                            exp_digits += 1;
                            j += 1;
                        }
                        _ => break,
                    }
                }
                if exp_digits == 0 {
                    break;
                }
                i = j;
            }
            _ => break,
        }
    }

    i
}

fn match_suffix(rest: &str) -> Option<(&'static str, usize)> {
    for &suffix in &ZIG_NUMERIC_SUFFIXES {
        if rest.starts_with(suffix) {
            return Some((suffix, suffix.len()));
        }
    }
    None
}

fn is_identifier_char(bytes: &[u8], index: usize) -> bool {
    if index >= bytes.len() {
        return false;
    }
    let ch = bytes[index] as char;
    ch.is_ascii_alphanumeric() || ch == '_'
}
