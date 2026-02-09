use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use anyhow::{Context, Result};
use tempfile::{Builder, TempDir};

use super::{
    ExecutionOutcome, ExecutionPayload, LanguageEngine, LanguageSession, cache_store,
    execution_timeout, hash_source, run_version_command, try_cached_execution, wait_with_timeout,
};

pub struct RustEngine {
    compiler: Option<PathBuf>,
}

impl Default for RustEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl RustEngine {
    pub fn new() -> Self {
        Self {
            compiler: resolve_rustc_binary(),
        }
    }

    fn ensure_compiler(&self) -> Result<&Path> {
        self.compiler.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "Rust support requires the `rustc` executable. Install it via Rustup and ensure it is on your PATH."
            )
        })
    }

    fn compile(&self, source: &Path, output: &Path) -> Result<std::process::Output> {
        let compiler = self.ensure_compiler()?;
        let mut cmd = Command::new(compiler);
        cmd.arg("--color=never")
            .arg("--edition=2021")
            .arg("--crate-name")
            .arg("run_snippet")
            .arg(source)
            .arg("-o")
            .arg(output);
        cmd.output()
            .with_context(|| format!("failed to invoke rustc at {}", compiler.display()))
    }

    fn run_binary(&self, binary: &Path, args: &[String]) -> Result<std::process::Output> {
        let mut cmd = Command::new(binary);
        cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
        cmd.stdin(Stdio::inherit());
        let child = cmd
            .spawn()
            .with_context(|| format!("failed to execute compiled binary {}", binary.display()))?;
        wait_with_timeout(child, execution_timeout())
    }

    fn write_inline_source(&self, code: &str, dir: &Path) -> Result<PathBuf> {
        let source_path = dir.join("main.rs");
        std::fs::write(&source_path, code).with_context(|| {
            format!(
                "failed to write temporary Rust source to {}",
                source_path.display()
            )
        })?;
        Ok(source_path)
    }

    fn tmp_binary_path(dir: &Path) -> PathBuf {
        let mut path = dir.join("run_rust_binary");
        if let Some(ext) = std::env::consts::EXE_SUFFIX.strip_prefix('.') {
            if !ext.is_empty() {
                path.set_extension(ext);
            }
        } else if !std::env::consts::EXE_SUFFIX.is_empty() {
            path = PathBuf::from(format!(
                "{}{}",
                path.display(),
                std::env::consts::EXE_SUFFIX
            ));
        }
        path
    }
}

impl LanguageEngine for RustEngine {
    fn id(&self) -> &'static str {
        "rust"
    }

    fn display_name(&self) -> &'static str {
        "Rust"
    }

    fn aliases(&self) -> &[&'static str] {
        &["rs"]
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

    fn toolchain_version(&self) -> Result<Option<String>> {
        let compiler = self.ensure_compiler()?;
        let mut cmd = Command::new(compiler);
        cmd.arg("--version");
        let context = format!("{}", compiler.display());
        run_version_command(cmd, &context)
    }

    fn execute(&self, payload: &ExecutionPayload) -> Result<ExecutionOutcome> {
        // Try cache for inline/stdin payloads
        let args = payload.args();

        if let Some(code) = match payload {
            ExecutionPayload::Inline { code, .. } | ExecutionPayload::Stdin { code, .. } => {
                Some(code.as_str())
            }
            _ => None,
        } {
            let src_hash = hash_source(code);
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
            .prefix("run-rust")
            .tempdir()
            .context("failed to create temporary directory for rust build")?;
        let dir_path = temp_dir.path();

        let (source_path, cleanup_source, cache_key): (PathBuf, bool, Option<u64>) = match payload {
            ExecutionPayload::Inline { code, .. } => {
                let h = hash_source(code);
                (self.write_inline_source(code, dir_path)?, true, Some(h))
            }
            ExecutionPayload::Stdin { code, .. } => {
                let h = hash_source(code);
                (self.write_inline_source(code, dir_path)?, true, Some(h))
            }
            ExecutionPayload::File { path, .. } => (path.clone(), false, None),
        };

        let binary_path = Self::tmp_binary_path(dir_path);
        let start = Instant::now();

        let compile_output = self.compile(&source_path, &binary_path)?;
        if !compile_output.status.success() {
            let stdout = String::from_utf8_lossy(&compile_output.stdout).into_owned();
            let stderr = String::from_utf8_lossy(&compile_output.stderr).into_owned();
            return Ok(ExecutionOutcome {
                language: self.id().to_string(),
                exit_code: compile_output.status.code(),
                stdout,
                stderr,
                duration: start.elapsed(),
            });
        }

        // Store in cache before running
        if let Some(h) = cache_key {
            cache_store(h, &binary_path);
        }

        let runtime_output = self.run_binary(&binary_path, args)?;
        let outcome = ExecutionOutcome {
            language: self.id().to_string(),
            exit_code: runtime_output.status.code(),
            stdout: String::from_utf8_lossy(&runtime_output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&runtime_output.stderr).into_owned(),
            duration: start.elapsed(),
        };

        if cleanup_source {
            let _ = std::fs::remove_file(&source_path);
        }
        let _ = std::fs::remove_file(&binary_path);

        Ok(outcome)
    }

    fn start_session(&self) -> Result<Box<dyn LanguageSession>> {
        let compiler = self.ensure_compiler()?.to_path_buf();
        let session = RustSession::new(compiler)?;
        Ok(Box::new(session))
    }
}

struct RustSession {
    compiler: PathBuf,
    workspace: TempDir,
    items: Vec<String>,
    statements: Vec<String>,
    last_stdout: String,
    last_stderr: String,
}

enum RustSnippetKind {
    Item,
    Statement,
}

impl RustSession {
    fn new(compiler: PathBuf) -> Result<Self> {
        let workspace = TempDir::new().context("failed to create Rust session workspace")?;
        let session = Self {
            compiler,
            workspace,
            items: Vec::new(),
            statements: Vec::new(),
            last_stdout: String::new(),
            last_stderr: String::new(),
        };
        session.persist_source()?;
        Ok(session)
    }

    fn language_id(&self) -> &str {
        "rust"
    }

    fn source_path(&self) -> PathBuf {
        self.workspace.path().join("session.rs")
    }

    fn binary_path(&self) -> PathBuf {
        RustEngine::tmp_binary_path(self.workspace.path())
    }

    fn persist_source(&self) -> Result<()> {
        let source = self.render_source();
        fs::write(self.source_path(), source)
            .with_context(|| "failed to write Rust session source".to_string())
    }

    fn render_source(&self) -> String {
        let mut source = String::from(
            r#"#![allow(unused_variables, unused_assignments, unused_mut, dead_code, unused_imports)]
use std::fmt::Debug;

fn __print<T: Debug>(value: T) {
    println!("{:?}", value);
}

"#,
        );

        for item in &self.items {
            source.push_str(item);
            if !item.ends_with('\n') {
                source.push('\n');
            }
            source.push('\n');
        }

        source.push_str("fn main() {\n");
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
        source.push_str("}\n");

        source
    }

    fn compile(&self, source: &Path, output: &Path) -> Result<std::process::Output> {
        let mut cmd = Command::new(&self.compiler);
        cmd.arg("--color=never")
            .arg("--edition=2021")
            .arg("--crate-name")
            .arg("run_snippet")
            .arg(source)
            .arg("-o")
            .arg(output);
        cmd.output()
            .with_context(|| format!("failed to invoke rustc at {}", self.compiler.display()))
    }

    fn run_binary(&self, binary: &Path) -> Result<std::process::Output> {
        let mut cmd = Command::new(binary);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        cmd.output().with_context(|| {
            format!(
                "failed to execute compiled Rust session binary {}",
                binary.display()
            )
        })
    }

    fn run_standalone_program(&mut self, code: &str) -> Result<ExecutionOutcome> {
        let start = Instant::now();
        let source_path = self.workspace.path().join("standalone.rs");
        fs::write(&source_path, code)
            .with_context(|| "failed to write standalone Rust source".to_string())?;

        let binary_path = self.binary_path();
        let compile_output = self.compile(&source_path, &binary_path)?;
        if !compile_output.status.success() {
            let outcome = ExecutionOutcome {
                language: self.language_id().to_string(),
                exit_code: compile_output.status.code(),
                stdout: String::from_utf8_lossy(&compile_output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&compile_output.stderr).into_owned(),
                duration: start.elapsed(),
            };
            let _ = fs::remove_file(&source_path);
            let _ = fs::remove_file(&binary_path);
            return Ok(outcome);
        }

        let runtime_output = self.run_binary(&binary_path)?;
        let outcome = ExecutionOutcome {
            language: self.language_id().to_string(),
            exit_code: runtime_output.status.code(),
            stdout: String::from_utf8_lossy(&runtime_output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&runtime_output.stderr).into_owned(),
            duration: start.elapsed(),
        };

        let _ = fs::remove_file(&source_path);
        let _ = fs::remove_file(&binary_path);

        Ok(outcome)
    }

    fn add_snippet(&mut self, code: &str) -> RustSnippetKind {
        let trimmed = code.trim();
        if trimmed.is_empty() {
            return RustSnippetKind::Statement;
        }

        if is_item_snippet(trimmed) {
            let mut snippet = code.to_string();
            if !snippet.ends_with('\n') {
                snippet.push('\n');
            }
            self.items.push(snippet);
            RustSnippetKind::Item
        } else {
            let stored = if should_treat_as_expression(trimmed) {
                wrap_expression(trimmed)
            } else {
                let mut snippet = code.to_string();
                if !snippet.ends_with('\n') {
                    snippet.push('\n');
                }
                snippet
            };
            self.statements.push(stored);
            RustSnippetKind::Statement
        }
    }

    fn rollback(&mut self, kind: RustSnippetKind) -> Result<()> {
        match kind {
            RustSnippetKind::Item => {
                self.items.pop();
            }
            RustSnippetKind::Statement => {
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

    fn run_snippet(&mut self, code: &str) -> Result<(ExecutionOutcome, bool)> {
        let start = Instant::now();
        let kind = self.add_snippet(code);
        self.persist_source()?;

        let source_path = self.source_path();
        let binary_path = self.binary_path();

        let compile_output = self.compile(&source_path, &binary_path)?;
        if !compile_output.status.success() {
            self.rollback(kind)?;
            let outcome = ExecutionOutcome {
                language: self.language_id().to_string(),
                exit_code: compile_output.status.code(),
                stdout: String::from_utf8_lossy(&compile_output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&compile_output.stderr).into_owned(),
                duration: start.elapsed(),
            };
            let _ = fs::remove_file(&binary_path);
            return Ok((outcome, false));
        }

        let runtime_output = self.run_binary(&binary_path)?;
        let stdout_full = Self::normalize_output(&runtime_output.stdout);
        let stderr_full = Self::normalize_output(&runtime_output.stderr);

        let stdout = Self::diff_outputs(&self.last_stdout, &stdout_full);
        let stderr = Self::diff_outputs(&self.last_stderr, &stderr_full);
        let success = runtime_output.status.success();

        if success {
            self.last_stdout = stdout_full;
            self.last_stderr = stderr_full;
        } else {
            self.rollback(kind)?;
        }

        let outcome = ExecutionOutcome {
            language: self.language_id().to_string(),
            exit_code: runtime_output.status.code(),
            stdout,
            stderr,
            duration: start.elapsed(),
        };

        let _ = fs::remove_file(&binary_path);

        Ok((outcome, success))
    }
}

impl LanguageSession for RustSession {
    fn language_id(&self) -> &str {
        RustSession::language_id(self)
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

        if contains_main_definition(trimmed) {
            return self.run_standalone_program(code);
        }

        let (outcome, _) = self.run_snippet(code)?;
        Ok(outcome)
    }

    fn shutdown(&mut self) -> Result<()> {
        Ok(())
    }
}

fn resolve_rustc_binary() -> Option<PathBuf> {
    which::which("rustc").ok()
}

fn is_item_snippet(code: &str) -> bool {
    let mut trimmed = code.trim_start();
    if trimmed.is_empty() {
        return false;
    }

    if trimmed.starts_with("#[") || trimmed.starts_with("#!") {
        return true;
    }

    if trimmed.starts_with("pub ") {
        trimmed = trimmed[4..].trim_start();
    } else if trimmed.starts_with("pub(")
        && let Some(idx) = trimmed.find(')')
    {
        trimmed = trimmed[idx + 1..].trim_start();
    }

    let first_token = trimmed.split_whitespace().next().unwrap_or("");
    let keywords = [
        "fn",
        "struct",
        "enum",
        "trait",
        "impl",
        "mod",
        "use",
        "type",
        "const",
        "static",
        "macro_rules!",
        "extern",
    ];

    if keywords.iter().any(|kw| first_token.starts_with(kw)) {
        return true;
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
    const RESERVED: [&str; 11] = [
        "let ", "const ", "static ", "fn ", "struct ", "enum ", "impl", "trait ", "mod ", "while ",
        "for ",
    ];
    if RESERVED.iter().any(|kw| trimmed.starts_with(kw)) {
        return false;
    }
    if trimmed.starts_with("if ") || trimmed.starts_with("loop ") || trimmed.starts_with("match ") {
        return false;
    }
    if trimmed.starts_with("return ") {
        return false;
    }
    true
}

fn wrap_expression(code: &str) -> String {
    format!("__print({});\n", code)
}

fn contains_main_definition(code: &str) -> bool {
    let bytes = code.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    let mut in_line_comment = false;
    let mut block_depth = 0usize;
    let mut in_string = false;
    let mut in_char = false;

    while i < len {
        let byte = bytes[i];

        if in_line_comment {
            if byte == b'\n' {
                in_line_comment = false;
            }
            i += 1;
            continue;
        }

        if in_string {
            if byte == b'\\' {
                i = (i + 2).min(len);
                continue;
            }
            if byte == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }

        if in_char {
            if byte == b'\\' {
                i = (i + 2).min(len);
                continue;
            }
            if byte == b'\'' {
                in_char = false;
            }
            i += 1;
            continue;
        }

        if block_depth > 0 {
            if byte == b'/' && i + 1 < len && bytes[i + 1] == b'*' {
                block_depth += 1;
                i += 2;
                continue;
            }
            if byte == b'*' && i + 1 < len && bytes[i + 1] == b'/' {
                block_depth -= 1;
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }

        match byte {
            b'/' if i + 1 < len && bytes[i + 1] == b'/' => {
                in_line_comment = true;
                i += 2;
                continue;
            }
            b'/' if i + 1 < len && bytes[i + 1] == b'*' => {
                block_depth = 1;
                i += 2;
                continue;
            }
            b'"' => {
                in_string = true;
                i += 1;
                continue;
            }
            b'\'' => {
                in_char = true;
                i += 1;
                continue;
            }
            b'f' if i + 1 < len && bytes[i + 1] == b'n' => {
                let mut prev_idx = i;
                let mut preceding_identifier = false;
                while prev_idx > 0 {
                    prev_idx -= 1;
                    let ch = bytes[prev_idx];
                    if ch.is_ascii_whitespace() {
                        continue;
                    }
                    if ch.is_ascii_alphanumeric() || ch == b'_' {
                        preceding_identifier = true;
                    }
                    break;
                }
                if preceding_identifier {
                    i += 1;
                    continue;
                }

                let mut j = i + 2;
                while j < len && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j + 4 > len || &bytes[j..j + 4] != b"main" {
                    i += 1;
                    continue;
                }

                let end_idx = j + 4;
                if end_idx < len {
                    let ch = bytes[end_idx];
                    if ch.is_ascii_alphanumeric() || ch == b'_' {
                        i += 1;
                        continue;
                    }
                }

                let mut after = end_idx;
                while after < len && bytes[after].is_ascii_whitespace() {
                    after += 1;
                }
                if after < len && bytes[after] != b'(' {
                    i += 1;
                    continue;
                }

                return true;
            }
            _ => {}
        }

        i += 1;
    }

    false
}
