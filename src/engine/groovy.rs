use std::borrow::Cow;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tempfile::{Builder, TempDir};

use super::{ExecutionOutcome, ExecutionPayload, LanguageEngine, LanguageSession};

pub struct GroovyEngine {
    executable: Option<PathBuf>,
}

impl GroovyEngine {
    pub fn new() -> Self {
        let executable = resolve_groovy_binary();
        Self { executable }
    }

    fn ensure_binary(&self) -> Result<&Path> {
        self.executable.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "Groovy support requires the `groovy` executable. Install it from https://groovy-lang.org/download.html and make sure it is available on your PATH."
            )
        })
    }
}

impl LanguageEngine for GroovyEngine {
    fn id(&self) -> &'static str {
        "groovy"
    }

    fn display_name(&self) -> &'static str {
        "Groovy"
    }

    fn aliases(&self) -> &[&'static str] {
        &["grv"]
    }

    fn supports_sessions(&self) -> bool {
        self.executable.is_some()
    }

    fn validate(&self) -> Result<()> {
        let binary = self.ensure_binary()?;
        let mut cmd = Command::new(binary);
        cmd.arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        cmd.status()
            .with_context(|| format!("failed to invoke {}", binary.display()))?
            .success()
            .then_some(())
            .ok_or_else(|| anyhow::anyhow!("{} is not executable", binary.display()))
    }

    fn execute(&self, payload: &ExecutionPayload) -> Result<ExecutionOutcome> {
        let binary = self.ensure_binary()?;
        let start = Instant::now();
        let output = match payload {
            ExecutionPayload::Inline { code } => {
                let prepared = prepare_groovy_source(code);
                let mut cmd = Command::new(binary);
                cmd.arg("-e").arg(prepared.as_ref());
                cmd.stdin(Stdio::inherit());
                cmd.output().with_context(|| {
                    format!(
                        "failed to execute {} for inline Groovy snippet",
                        binary.display()
                    )
                })
            }
            ExecutionPayload::File { path } => {
                let mut cmd = Command::new(binary);
                cmd.arg(path);
                cmd.stdin(Stdio::inherit());
                cmd.output().with_context(|| {
                    format!(
                        "failed to execute {} for Groovy script {}",
                        binary.display(),
                        path.display()
                    )
                })
            }
            ExecutionPayload::Stdin { code } => {
                let mut script = Builder::new()
                    .prefix("run-groovy-stdin")
                    .suffix(".groovy")
                    .tempfile()
                    .context("failed to create temporary Groovy script for stdin input")?;
                let mut prepared = prepare_groovy_source(code).into_owned();
                if !prepared.ends_with('\n') {
                    prepared.push('\n');
                }
                script
                    .write_all(prepared.as_bytes())
                    .context("failed to write piped Groovy source")?;
                script.flush()?;

                let script_path = script.path().to_path_buf();
                let mut cmd = Command::new(binary);
                cmd.arg(&script_path);
                cmd.stdin(Stdio::null());
                let output = cmd.output().with_context(|| {
                    format!(
                        "failed to execute {} for Groovy stdin script {}",
                        binary.display(),
                        script_path.display()
                    )
                })?;
                drop(script);
                Ok(output)
            }
        }?;

        Ok(ExecutionOutcome {
            language: self.id().to_string(),
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            duration: start.elapsed(),
        })
    }

    fn start_session(&self) -> Result<Box<dyn LanguageSession>> {
        let executable = self.ensure_binary()?.to_path_buf();
        Ok(Box::new(GroovySession::new(executable)?))
    }
}

fn resolve_groovy_binary() -> Option<PathBuf> {
    which::which("groovy").ok()
}

struct GroovySession {
    executable: PathBuf,
    dir: TempDir,
    source_path: PathBuf,
    statements: Vec<String>,
    previous_stdout: String,
    previous_stderr: String,
}

impl GroovySession {
    fn new(executable: PathBuf) -> Result<Self> {
        let dir = Builder::new()
            .prefix("run-groovy-repl")
            .tempdir()
            .context("failed to create temporary directory for groovy repl")?;
        let source_path = dir.path().join("session.groovy");
        fs::write(&source_path, "// Groovy REPL session\n").with_context(|| {
            format!(
                "failed to initialize generated groovy session source at {}",
                source_path.display()
            )
        })?;

        Ok(Self {
            executable,
            dir,
            source_path,
            statements: Vec::new(),
            previous_stdout: String::new(),
            previous_stderr: String::new(),
        })
    }

    fn render_source(&self) -> String {
        let mut source = String::from("// Generated by run Groovy REPL\n");
        for snippet in &self.statements {
            source.push_str(snippet);
            if !snippet.ends_with('\n') {
                source.push('\n');
            }
        }
        source
    }

    fn write_source(&self, contents: &str) -> Result<()> {
        fs::write(&self.source_path, contents).with_context(|| {
            format!(
                "failed to write generated Groovy REPL source to {}",
                self.source_path.display()
            )
        })
    }

    fn run_current(&mut self, start: Instant) -> Result<(ExecutionOutcome, bool)> {
        let source = self.render_source();
        self.write_source(&source)?;

        let output = self.run_script()?;
        let stdout_full = normalize_output(&output.stdout);
        let stderr_full = normalize_output(&output.stderr);

        let stdout_delta = diff_output(&self.previous_stdout, &stdout_full);
        let stderr_delta = diff_output(&self.previous_stderr, &stderr_full);

        let success = output.status.success();
        if success {
            self.previous_stdout = stdout_full;
            self.previous_stderr = stderr_full;
        }

        let outcome = ExecutionOutcome {
            language: "groovy".to_string(),
            exit_code: output.status.code(),
            stdout: stdout_delta,
            stderr: stderr_delta,
            duration: start.elapsed(),
        };

        Ok((outcome, success))
    }

    fn run_script(&self) -> Result<std::process::Output> {
        let mut cmd = Command::new(&self.executable);
        cmd.arg(&self.source_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(self.dir.path());
        cmd.output().with_context(|| {
            format!(
                "failed to run groovy session script {} with {}",
                self.source_path.display(),
                self.executable.display()
            )
        })
    }

    fn run_snippet(&mut self, snippet: String) -> Result<ExecutionOutcome> {
        self.statements.push(snippet);
        let start = Instant::now();
        let (outcome, success) = self.run_current(start)?;
        if !success {
            let _ = self.statements.pop();
            let source = self.render_source();
            self.write_source(&source)?;
        }
        Ok(outcome)
    }

    fn reset_state(&mut self) -> Result<()> {
        self.statements.clear();
        self.previous_stdout.clear();
        self.previous_stderr.clear();
        let source = self.render_source();
        self.write_source(&source)
    }
}

impl LanguageSession for GroovySession {
    fn language_id(&self) -> &str {
        "groovy"
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
                    "Groovy commands:\n  :reset — clear session state\n  :help  — show this message\n"
                        .to_string(),
                stderr: String::new(),
                duration: Duration::default(),
            });
        }

        if should_treat_as_expression(trimmed) {
            let snippet = wrap_expression(trimmed, self.statements.len());
            let outcome = self.run_snippet(snippet)?;
            if outcome.exit_code.unwrap_or(0) == 0 {
                return Ok(outcome);
            }
        }

        let snippet = ensure_trailing_newline(code);
        self.run_snippet(snippet)
    }

    fn shutdown(&mut self) -> Result<()> {
        // TempDir cleanup handled automatically.
        Ok(())
    }
}

fn ensure_trailing_newline(code: &str) -> String {
    let mut owned = code.to_string();
    if !owned.ends_with('\n') {
        owned.push('\n');
    }
    owned
}

fn wrap_expression(code: &str, index: usize) -> String {
    format!("def __run_value_{index} = ({code});\nprintln(__run_value_{index});\n")
}

fn should_treat_as_expression(code: &str) -> bool {
    let trimmed = code.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.contains('\n') {
        return false;
    }
    if trimmed.ends_with('{') || trimmed.ends_with('}') {
        return false;
    }

    let lowered = trimmed.to_ascii_lowercase();
    const STATEMENT_PREFIXES: [&str; 17] = [
        "import ",
        "package ",
        "class ",
        "interface ",
        "enum ",
        "trait ",
        "def ",
        "if ",
        "for ",
        "while ",
        "switch ",
        "case ",
        "try",
        "catch",
        "finally",
        "return ",
        "throw ",
    ];
    if STATEMENT_PREFIXES
        .iter()
        .any(|prefix| lowered.starts_with(prefix))
    {
        return false;
    }

    if trimmed.starts_with("//") {
        return false;
    }

    if lowered.starts_with("println")
        || lowered.starts_with("print ")
        || lowered.starts_with("print(")
    {
        return false;
    }

    if trimmed.contains('=')
        && !trimmed.contains("==")
        && !trimmed.contains("!=")
        && !trimmed.contains(">=")
        && !trimmed.contains("<=")
        && !trimmed.contains("=>")
    {
        return false;
    }

    true
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

fn prepare_groovy_source(code: &str) -> Cow<'_, str> {
    if let Some(expr) = extract_tail_expression(code) {
        let mut script = code.to_string();
        if !script.ends_with('\n') {
            script.push('\n');
        }
        script.push_str(&format!("println({expr});\n"));
        Cow::Owned(script)
    } else {
        Cow::Borrowed(code)
    }
}

fn extract_tail_expression(source: &str) -> Option<String> {
    for line in source.lines().rev() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("//") {
            continue;
        }
        let without_comment = strip_inline_comment(trimmed).trim();
        if without_comment.is_empty() {
            continue;
        }
        if should_treat_as_expression(without_comment) {
            return Some(without_comment.to_string());
        }
        break;
    }
    None
}

fn strip_inline_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut in_single = false;
    let mut in_double = false;
    let mut escape = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if escape {
            escape = false;
            i += 1;
            continue;
        }
        match b {
            b'\\' => {
                escape = true;
            }
            b'\'' if !in_double => {
                in_single = !in_single;
            }
            b'"' if !in_single => {
                in_double = !in_double;
            }
            b'/' if !in_single && !in_double => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                    return &line[..i];
                }
            }
            _ => {}
        }
        i += 1;
    }
    line
}
