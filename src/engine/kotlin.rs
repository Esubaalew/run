use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tempfile::{Builder, TempDir};

use super::{ExecutionOutcome, ExecutionPayload, LanguageEngine, LanguageSession, hash_source};

pub struct KotlinEngine {
    compiler: Option<PathBuf>,
    java: Option<PathBuf>,
}

impl KotlinEngine {
    pub fn new() -> Self {
        Self {
            compiler: resolve_kotlinc_binary(),
            java: resolve_java_binary(),
        }
    }

    fn ensure_compiler(&self) -> Result<&Path> {
        self.compiler.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "Kotlin support requires the `kotlinc` compiler. Install it from https://kotlinlang.org/docs/command-line.html and ensure it is on your PATH."
            )
        })
    }

    fn ensure_java(&self) -> Result<&Path> {
        self.java.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "Kotlin execution requires a `java` runtime. Install a JDK and ensure `java` is on your PATH."
            )
        })
    }

    fn write_inline_source(&self, code: &str, dir: &Path) -> Result<PathBuf> {
        let source_path = dir.join("Main.kt");
        let wrapped = wrap_inline_kotlin(code);
        std::fs::write(&source_path, wrapped).with_context(|| {
            format!(
                "failed to write temporary Kotlin source to {}",
                source_path.display()
            )
        })?;
        Ok(source_path)
    }

    fn copy_source(&self, original: &Path, dir: &Path) -> Result<PathBuf> {
        let file_name = original
            .file_name()
            .map(|f| f.to_owned())
            .ok_or_else(|| anyhow::anyhow!("invalid Kotlin source path"))?;
        let target = dir.join(&file_name);
        std::fs::copy(original, &target).with_context(|| {
            format!(
                "failed to copy Kotlin source from {} to {}",
                original.display(),
                target.display()
            )
        })?;
        Ok(target)
    }

    fn compile(&self, source: &Path, jar: &Path) -> Result<std::process::Output> {
        let compiler = self.ensure_compiler()?;
        invoke_kotlin_compiler(compiler, source, jar)
    }

    fn run(&self, jar: &Path) -> Result<std::process::Output> {
        let java = self.ensure_java()?;
        run_kotlin_jar(java, jar)
    }
}

impl LanguageEngine for KotlinEngine {
    fn id(&self) -> &'static str {
        "kotlin"
    }

    fn display_name(&self) -> &'static str {
        "Kotlin"
    }

    fn aliases(&self) -> &[&'static str] {
        &["kt"]
    }

    fn supports_sessions(&self) -> bool {
        self.compiler.is_some() && self.java.is_some()
    }

    fn validate(&self) -> Result<()> {
        let compiler = self.ensure_compiler()?;
        let mut compile_check = Command::new(compiler);
        compile_check
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        compile_check
            .status()
            .with_context(|| format!("failed to invoke {}", compiler.display()))?
            .success()
            .then_some(())
            .ok_or_else(|| anyhow::anyhow!("{} is not executable", compiler.display()))?;

        let java = self.ensure_java()?;
        let mut java_check = Command::new(java);
        java_check
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        java_check
            .status()
            .with_context(|| format!("failed to invoke {}", java.display()))?
            .success()
            .then_some(())
            .ok_or_else(|| anyhow::anyhow!("{} is not executable", java.display()))?;

        Ok(())
    }

    fn execute(&self, payload: &ExecutionPayload) -> Result<ExecutionOutcome> {
        // Check jar cache for inline/stdin payloads
        if let Some(code) = match payload {
            ExecutionPayload::Inline { code } | ExecutionPayload::Stdin { code } => {
                Some(code.as_str())
            }
            _ => None,
        } {
            let wrapped = wrap_inline_kotlin(code);
            let src_hash = hash_source(&wrapped);
            let cached_jar = std::env::temp_dir()
                .join("run-compile-cache")
                .join(format!("kotlin-{:016x}.jar", src_hash));
            if cached_jar.exists() {
                let start = Instant::now();
                if let Ok(output) = self.run(&cached_jar) {
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

        let temp_dir = Builder::new()
            .prefix("run-kotlin")
            .tempdir()
            .context("failed to create temporary directory for kotlin build")?;
        let dir_path = temp_dir.path();

        let source_path = match payload {
            ExecutionPayload::Inline { code } | ExecutionPayload::Stdin { code } => {
                self.write_inline_source(code, dir_path)?
            }
            ExecutionPayload::File { path } => self.copy_source(path, dir_path)?,
        };

        let jar_path = dir_path.join("snippet.jar");
        let start = Instant::now();

        let compile_output = self.compile(&source_path, &jar_path)?;
        if !compile_output.status.success() {
            return Ok(ExecutionOutcome {
                language: self.id().to_string(),
                exit_code: compile_output.status.code(),
                stdout: String::from_utf8_lossy(&compile_output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&compile_output.stderr).into_owned(),
                duration: start.elapsed(),
            });
        }

        // Cache the compiled jar
        if let Some(code) = match payload {
            ExecutionPayload::Inline { code } | ExecutionPayload::Stdin { code } => {
                Some(code.as_str())
            }
            _ => None,
        } {
            let wrapped = wrap_inline_kotlin(code);
            let src_hash = hash_source(&wrapped);
            let cache_dir = std::env::temp_dir().join("run-compile-cache");
            let _ = std::fs::create_dir_all(&cache_dir);
            let cached_jar = cache_dir.join(format!("kotlin-{:016x}.jar", src_hash));
            let _ = std::fs::copy(&jar_path, &cached_jar);
        }

        let run_output = self.run(&jar_path)?;
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
        let java = self.ensure_java()?.to_path_buf();

        let dir = Builder::new()
            .prefix("run-kotlin-repl")
            .tempdir()
            .context("failed to create temporary directory for kotlin repl")?;
        let dir_path = dir.path();

        let source_path = dir_path.join("Session.kt");
        let jar_path = dir_path.join("session.jar");
        fs::write(&source_path, "// Kotlin REPL session\n").with_context(|| {
            format!(
                "failed to initialize Kotlin session source at {}",
                source_path.display()
            )
        })?;

        Ok(Box::new(KotlinSession {
            compiler,
            java,
            _dir: dir,
            source_path,
            jar_path,
            definitions: Vec::new(),
            statements: Vec::new(),
            previous_stdout: String::new(),
            previous_stderr: String::new(),
        }))
    }
}

fn resolve_kotlinc_binary() -> Option<PathBuf> {
    which::which("kotlinc").ok()
}

fn resolve_java_binary() -> Option<PathBuf> {
    which::which("java").ok()
}

fn wrap_inline_kotlin(body: &str) -> String {
    if body.contains("fun main") {
        return body.to_string();
    }

    let mut header_lines = Vec::new();
    let mut rest_lines = Vec::new();
    let mut in_header = true;

    for line in body.lines() {
        let trimmed = line.trim_start();
        if in_header && (trimmed.starts_with("import ") || trimmed.starts_with("package ")) {
            header_lines.push(line);
            continue;
        }
        in_header = false;
        rest_lines.push(line);
    }

    let mut result = String::new();
    if !header_lines.is_empty() {
        for hl in header_lines {
            result.push_str(hl);
            if !hl.ends_with('\n') {
                result.push('\n');
            }
        }
        result.push('\n');
    }

    result.push_str("fun main() {\n");
    for line in rest_lines {
        if line.trim().is_empty() {
            result.push_str("    \n");
        } else {
            result.push_str("    ");
            result.push_str(line);
            result.push('\n');
        }
    }
    result.push_str("}\n");
    result
}

fn contains_main_function(code: &str) -> bool {
    code.lines()
        .any(|line| line.trim_start().starts_with("fun main"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SnippetKind {
    Definition,
    Statement,
    Expression,
}

fn classify_snippet(code: &str) -> SnippetKind {
    let trimmed = code.trim();
    if trimmed.is_empty() {
        return SnippetKind::Statement;
    }

    const DEF_PREFIXES: [&str; 13] = [
        "fun ",
        "class ",
        "object ",
        "interface ",
        "enum ",
        "sealed ",
        "data class ",
        "annotation ",
        "typealias ",
        "package ",
        "import ",
        "val ",
        "var ",
    ];
    if DEF_PREFIXES
        .iter()
        .any(|prefix| trimmed.starts_with(prefix))
    {
        return SnippetKind::Definition;
    }

    if trimmed.starts_with('@') {
        return SnippetKind::Definition;
    }

    if is_kotlin_expression(trimmed) {
        return SnippetKind::Expression;
    }

    SnippetKind::Statement
}

fn is_kotlin_expression(code: &str) -> bool {
    if code.contains('\n') {
        return false;
    }
    if code.ends_with(';') {
        return false;
    }

    let lowered = code.trim_start().to_ascii_lowercase();
    const DISALLOWED_PREFIXES: [&str; 14] = [
        "while ", "for ", "do ", "try ", "catch", "finally", "return ", "throw ", "break",
        "continue", "val ", "var ", "fun ", "class ",
    ];
    if DISALLOWED_PREFIXES
        .iter()
        .any(|prefix| lowered.starts_with(prefix))
    {
        return false;
    }

    if code.starts_with("print") {
        return false;
    }

    if code == "true" || code == "false" {
        return true;
    }
    if code.parse::<f64>().is_ok() {
        return true;
    }
    if code.starts_with('"') && code.ends_with('"') && code.len() >= 2 {
        return true;
    }
    if code.contains("==")
        || code.contains("!=")
        || code.contains("<=")
        || code.contains(">=")
        || code.contains("&&")
        || code.contains("||")
    {
        return true;
    }
    const ASSIGN_OPS: [&str; 7] = ["=", "+=", "-=", "*=", "/=", "%=", "= "];
    if ASSIGN_OPS.iter().any(|op| code.contains(op))
        && !code.contains("==")
        && !code.contains("!=")
        && !code.contains(">=")
        && !code.contains("<=")
        && !code.contains("=>")
    {
        return false;
    }

    if code.chars().any(|c| "+-*/%<>^|&".contains(c)) {
        return true;
    }

    if code
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '$')
    {
        return true;
    }

    code.contains('(') && code.contains(')')
}

fn wrap_kotlin_expression(code: &str, index: usize) -> String {
    format!("val __repl_val_{index} = ({code})\nprintln(__repl_val_{index})\n")
}

fn ensure_trailing_newline(code: &str) -> String {
    let mut owned = code.to_string();
    if !owned.ends_with('\n') {
        owned.push('\n');
    }
    owned
}

fn diff_output(previous: &str, current: &str) -> String {
    if let Some(stripped) = current.strip_prefix(previous) {
        stripped.to_string()
    } else {
        current.to_string()
    }
}

fn normalize_output(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .replace("\r\n", "\n")
        .replace('\r', "")
}

fn invoke_kotlin_compiler(
    compiler: &Path,
    source: &Path,
    jar: &Path,
) -> Result<std::process::Output> {
    let mut cmd = Command::new(compiler);
    cmd.arg(source)
        .arg("-include-runtime")
        .arg("-d")
        .arg(jar)
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

fn run_kotlin_jar(java: &Path, jar: &Path) -> Result<std::process::Output> {
    let mut cmd = Command::new(java);
    cmd.arg("-jar")
        .arg(jar)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd.stdin(Stdio::inherit());
    cmd.output().with_context(|| {
        format!(
            "failed to execute {} -jar {}",
            java.display(),
            jar.display()
        )
    })
}
struct KotlinSession {
    compiler: PathBuf,
    java: PathBuf,
    _dir: TempDir,
    source_path: PathBuf,
    jar_path: PathBuf,
    definitions: Vec<String>,
    statements: Vec<String>,
    previous_stdout: String,
    previous_stderr: String,
}

impl KotlinSession {
    fn render_prelude(&self) -> String {
        let mut source = String::from("import kotlin.math.*\n\n");
        for def in &self.definitions {
            source.push_str(def);
            if !def.ends_with('\n') {
                source.push('\n');
            }
            source.push('\n');
        }
        source
    }

    fn render_source(&self) -> String {
        let mut source = self.render_prelude();
        source.push_str("fun main() {\n");
        for stmt in &self.statements {
            for line in stmt.lines() {
                source.push_str("    ");
                source.push_str(line);
                source.push('\n');
            }
            if !stmt.ends_with('\n') {
                source.push('\n');
            }
        }
        source.push_str("}\n");
        source
    }

    fn write_source(&self, contents: &str) -> Result<()> {
        fs::write(&self.source_path, contents).with_context(|| {
            format!(
                "failed to write generated Kotlin REPL source to {}",
                self.source_path.display()
            )
        })
    }

    fn compile_and_run(&mut self) -> Result<(std::process::Output, Duration)> {
        let start = Instant::now();
        let source = self.render_source();
        self.write_source(&source)?;
        let compile_output =
            invoke_kotlin_compiler(&self.compiler, &self.source_path, &self.jar_path)?;
        if !compile_output.status.success() {
            return Ok((compile_output, start.elapsed()));
        }
        let run_output = run_kotlin_jar(&self.java, &self.jar_path)?;
        Ok((run_output, start.elapsed()))
    }

    fn diff_outputs(
        &mut self,
        output: &std::process::Output,
        duration: Duration,
    ) -> ExecutionOutcome {
        let stdout_full = normalize_output(&output.stdout);
        let stderr_full = normalize_output(&output.stderr);

        let stdout_delta = diff_output(&self.previous_stdout, &stdout_full);
        let stderr_delta = diff_output(&self.previous_stderr, &stderr_full);

        if output.status.success() {
            self.previous_stdout = stdout_full;
            self.previous_stderr = stderr_full;
        }

        ExecutionOutcome {
            language: "kotlin".to_string(),
            exit_code: output.status.code(),
            stdout: stdout_delta,
            stderr: stderr_delta,
            duration,
        }
    }

    fn add_definition(&mut self, snippet: String) {
        self.definitions.push(snippet);
    }

    fn add_statement(&mut self, snippet: String) {
        self.statements.push(snippet);
    }

    fn remove_last_definition(&mut self) {
        let _ = self.definitions.pop();
    }

    fn remove_last_statement(&mut self) {
        let _ = self.statements.pop();
    }

    fn reset_state(&mut self) -> Result<()> {
        self.definitions.clear();
        self.statements.clear();
        self.previous_stdout.clear();
        self.previous_stderr.clear();
        let source = self.render_source();
        self.write_source(&source)
    }

    fn run_standalone_program(&mut self, code: &str) -> Result<ExecutionOutcome> {
        let start = Instant::now();
        let mut source = self.render_prelude();
        if !source.ends_with('\n') {
            source.push('\n');
        }
        source.push_str(code);
        if !code.ends_with('\n') {
            source.push('\n');
        }

        let standalone_path = self
            .source_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("standalone.kt");
        fs::write(&standalone_path, &source).with_context(|| {
            format!(
                "failed to write standalone Kotlin source to {}",
                standalone_path.display()
            )
        })?;

        let compile_output =
            invoke_kotlin_compiler(&self.compiler, &standalone_path, &self.jar_path)?;
        if !compile_output.status.success() {
            return Ok(ExecutionOutcome {
                language: "kotlin".to_string(),
                exit_code: compile_output.status.code(),
                stdout: normalize_output(&compile_output.stdout),
                stderr: normalize_output(&compile_output.stderr),
                duration: start.elapsed(),
            });
        }

        let run_output = run_kotlin_jar(&self.java, &self.jar_path)?;
        Ok(ExecutionOutcome {
            language: "kotlin".to_string(),
            exit_code: run_output.status.code(),
            stdout: normalize_output(&run_output.stdout),
            stderr: normalize_output(&run_output.stderr),
            duration: start.elapsed(),
        })
    }
}

impl LanguageSession for KotlinSession {
    fn language_id(&self) -> &str {
        "kotlin"
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
            self.reset_state()?;
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
                    "Kotlin commands:\n  :reset - clear session state\n  :help  - show this message\n"
                        .to_string(),
                stderr: String::new(),
                duration: Duration::default(),
            });
        }

        if contains_main_function(code) {
            return self.run_standalone_program(code);
        }

        let classification = classify_snippet(trimmed);
        match classification {
            SnippetKind::Definition => {
                self.add_definition(code.to_string());
                let (output, duration) = self.compile_and_run()?;
                if !output.status.success() {
                    self.remove_last_definition();
                }
                Ok(self.diff_outputs(&output, duration))
            }
            SnippetKind::Expression => {
                let wrapped = wrap_kotlin_expression(trimmed, self.statements.len());
                self.add_statement(wrapped);
                let (output, duration) = self.compile_and_run()?;
                if !output.status.success() {
                    self.remove_last_statement();
                }
                Ok(self.diff_outputs(&output, duration))
            }
            SnippetKind::Statement => {
                let stmt = ensure_trailing_newline(code);
                self.add_statement(stmt);
                let (output, duration) = self.compile_and_run()?;
                if !output.status.success() {
                    self.remove_last_statement();
                }
                Ok(self.diff_outputs(&output, duration))
            }
        }
    }

    fn shutdown(&mut self) -> Result<()> {
        Ok(())
    }
}
