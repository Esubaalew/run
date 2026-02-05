use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tempfile::{Builder, TempDir};

use super::{ExecutionOutcome, ExecutionPayload, LanguageEngine, LanguageSession};

pub struct PhpEngine {
    interpreter: Option<PathBuf>,
}

impl PhpEngine {
    pub fn new() -> Self {
        Self {
            interpreter: resolve_php_binary(),
        }
    }

    fn ensure_interpreter(&self) -> Result<&Path> {
        self.interpreter.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "PHP support requires the `php` CLI executable. Install PHP and ensure it is on your PATH."
            )
        })
    }

    fn write_temp_script(&self, code: &str) -> Result<(tempfile::TempDir, PathBuf)> {
        let dir = Builder::new()
            .prefix("run-php")
            .tempdir()
            .context("failed to create temporary directory for php source")?;
        let path = dir.path().join("snippet.php");
        let mut contents = code.to_string();
        if !contents.starts_with("<?php") {
            contents = format!("<?php\n{}", contents);
        }
        if !contents.ends_with('\n') {
            contents.push('\n');
        }
        std::fs::write(&path, contents).with_context(|| {
            format!("failed to write temporary PHP source to {}", path.display())
        })?;
        Ok((dir, path))
    }

    fn run_script(&self, script: &Path) -> Result<std::process::Output> {
        let interpreter = self.ensure_interpreter()?;
        let mut cmd = Command::new(interpreter);
        cmd.arg(script)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd.stdin(Stdio::inherit());
        if let Some(dir) = script.parent() {
            cmd.current_dir(dir);
        }
        cmd.output().with_context(|| {
            format!(
                "failed to execute {} with script {}",
                interpreter.display(),
                script.display()
            )
        })
    }
}

impl LanguageEngine for PhpEngine {
    fn id(&self) -> &'static str {
        "php"
    }

    fn display_name(&self) -> &'static str {
        "PHP"
    }

    fn aliases(&self) -> &[&'static str] {
        &[]
    }

    fn supports_sessions(&self) -> bool {
        self.interpreter.is_some()
    }

    fn validate(&self) -> Result<()> {
        let interpreter = self.ensure_interpreter()?;
        let mut cmd = Command::new(interpreter);
        cmd.arg("-v").stdout(Stdio::null()).stderr(Stdio::null());
        cmd.status()
            .with_context(|| format!("failed to invoke {}", interpreter.display()))?
            .success()
            .then_some(())
            .ok_or_else(|| anyhow::anyhow!("{} is not executable", interpreter.display()))
    }

    fn execute(&self, payload: &ExecutionPayload) -> Result<ExecutionOutcome> {
        let start = Instant::now();
        let (temp_dir, script_path) = match payload {
            ExecutionPayload::Inline { code } | ExecutionPayload::Stdin { code } => {
                let (dir, path) = self.write_temp_script(code)?;
                (Some(dir), path)
            }
            ExecutionPayload::File { path } => (None, path.clone()),
        };

        let output = self.run_script(&script_path)?;
        drop(temp_dir);

        Ok(ExecutionOutcome {
            language: self.id().to_string(),
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            duration: start.elapsed(),
        })
    }

    fn start_session(&self) -> Result<Box<dyn LanguageSession>> {
        let interpreter = self.ensure_interpreter()?.to_path_buf();
        let session = PhpSession::new(interpreter)?;
        Ok(Box::new(session))
    }
}

fn resolve_php_binary() -> Option<PathBuf> {
    which::which("php").ok()
}

const SESSION_MAIN_FILE: &str = "session.php";
const PHP_PROMPT_PREFIXES: &[&str] = &["php>>> ", "php>>>", "... ", "..."];

struct PhpSession {
    interpreter: PathBuf,
    workspace: TempDir,
    statements: Vec<String>,
    last_stdout: String,
    last_stderr: String,
}

impl PhpSession {
    fn new(interpreter: PathBuf) -> Result<Self> {
        let workspace = TempDir::new().context("failed to create PHP session workspace")?;
        let session = Self {
            interpreter,
            workspace,
            statements: Vec::new(),
            last_stdout: String::new(),
            last_stderr: String::new(),
        };
        session.persist_source()?;
        Ok(session)
    }

    fn language_id(&self) -> &str {
        "php"
    }

    fn source_path(&self) -> PathBuf {
        self.workspace.path().join(SESSION_MAIN_FILE)
    }

    fn persist_source(&self) -> Result<()> {
        let path = self.source_path();
        let source = self.render_source();
        fs::write(&path, source)
            .with_context(|| format!("failed to write PHP session source at {}", path.display()))
    }

    fn render_source(&self) -> String {
        let mut source = String::from("<?php\n");
        if self.statements.is_empty() {
            source.push_str("// session body\n");
        } else {
            for stmt in &self.statements {
                source.push_str(stmt);
                if !stmt.ends_with('\n') {
                    source.push('\n');
                }
            }
        }
        source
    }

    fn run_program(&self) -> Result<std::process::Output> {
        let mut cmd = Command::new(&self.interpreter);
        cmd.arg(SESSION_MAIN_FILE)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(self.workspace.path());
        cmd.output().with_context(|| {
            format!(
                "failed to execute {} for PHP session",
                self.interpreter.display()
            )
        })
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
}

impl LanguageSession for PhpSession {
    fn language_id(&self) -> &str {
        self.language_id()
    }

    fn eval(&mut self, code: &str) -> Result<ExecutionOutcome> {
        let trimmed = code.trim();

        if trimmed.eq_ignore_ascii_case(":reset") {
            self.statements.clear();
            self.last_stdout.clear();
            self.last_stderr.clear();
            self.persist_source()?;
            return Ok(ExecutionOutcome {
                language: self.language_id().to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                duration: Duration::default(),
            });
        }

        if trimmed.is_empty() {
            return Ok(ExecutionOutcome {
                language: self.language_id().to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                duration: Duration::default(),
            });
        }

        let mut statement = normalize_php_snippet(code);
        if statement.trim().is_empty() {
            return Ok(ExecutionOutcome {
                language: self.language_id().to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                duration: Duration::default(),
            });
        }

        if !statement.ends_with('\n') {
            statement.push('\n');
        }

        self.statements.push(statement);
        self.persist_source()?;

        let start = Instant::now();
        let output = self.run_program()?;
        let stdout_full = PhpSession::normalize_output(&output.stdout);
        let stderr_full = PhpSession::normalize_output(&output.stderr);
        let stdout = PhpSession::diff_outputs(&self.last_stdout, &stdout_full);
        let stderr = PhpSession::diff_outputs(&self.last_stderr, &stderr_full);
        let duration = start.elapsed();

        if output.status.success() {
            self.last_stdout = stdout_full;
            self.last_stderr = stderr_full;
            Ok(ExecutionOutcome {
                language: self.language_id().to_string(),
                exit_code: output.status.code(),
                stdout,
                stderr,
                duration,
            })
        } else {
            self.statements.pop();
            self.persist_source()?;
            Ok(ExecutionOutcome {
                language: self.language_id().to_string(),
                exit_code: output.status.code(),
                stdout,
                stderr,
                duration,
            })
        }
    }

    fn shutdown(&mut self) -> Result<()> {
        Ok(())
    }
}

fn strip_leading_php_prompt(line: &str) -> String {
    let without_bom = line.trim_start_matches('\u{feff}');
    let mut leading_len = 0;
    for (idx, ch) in without_bom.char_indices() {
        if ch == ' ' || ch == '\t' {
            leading_len = idx + ch.len_utf8();
        } else {
            break;
        }
    }
    let (leading_ws, rest) = without_bom.split_at(leading_len);
    for prefix in PHP_PROMPT_PREFIXES {
        if rest.starts_with(prefix) {
            return format!("{}{}", leading_ws, &rest[prefix.len()..]);
        }
    }
    without_bom.to_string()
}

fn normalize_php_snippet(code: &str) -> String {
    let mut lines: Vec<String> = code.lines().map(strip_leading_php_prompt).collect();

    while let Some(first) = lines.first() {
        let trimmed = first.trim();
        if trimmed.is_empty() {
            lines.remove(0);
            continue;
        }
        if trimmed.starts_with("<?php") {
            lines.remove(0);
            break;
        }
        if trimmed == "<?" {
            lines.remove(0);
            break;
        }
        break;
    }

    while let Some(last) = lines.last() {
        let trimmed = last.trim();
        if trimmed.is_empty() {
            lines.pop();
            continue;
        }
        if trimmed == "?>" {
            lines.pop();
            continue;
        }
        break;
    }

    if lines.is_empty() {
        String::new()
    } else {
        let mut result = lines.join("\n");
        if code.ends_with('\n') {
            result.push('\n');
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::{PhpSession, normalize_php_snippet};

    #[test]
    fn strips_prompt_prefixes() {
        let input = "php>>> echo 'hello';\n... echo 'world';\n";
        let normalized = normalize_php_snippet(input);
        assert_eq!(normalized, "echo 'hello';\necho 'world';\n");
    }

    #[test]
    fn preserves_indentation_after_prompt_removal() {
        let input = "    php>>> if (true) {\n    ...     echo 'ok';\n    ... }\n";
        let normalized = normalize_php_snippet(input);
        assert_eq!(normalized, "    if (true) {\n        echo 'ok';\n    }\n");
    }

    #[test]
    fn diff_outputs_appends_only_suffix() {
        let previous = "a\nb\n";
        let current = "a\nb\nc\n";
        assert_eq!(PhpSession::diff_outputs(previous, current), "c\n");

        let previous = "a\n";
        let current = "x\na\n";
        assert_eq!(PhpSession::diff_outputs(previous, current), "x\na\n");
    }
}
