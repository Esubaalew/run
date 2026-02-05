use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tempfile::{Builder, TempDir};

use super::{ExecutionOutcome, ExecutionPayload, LanguageEngine, LanguageSession};

pub struct BashEngine {
    executable: PathBuf,
}

impl BashEngine {
    pub fn new() -> Self {
        let executable = resolve_bash_binary();
        Self { executable }
    }

    fn binary(&self) -> &Path {
        &self.executable
    }

    fn run_command(&self) -> Command {
        Command::new(self.binary())
    }
}

impl LanguageEngine for BashEngine {
    fn id(&self) -> &'static str {
        "bash"
    }

    fn display_name(&self) -> &'static str {
        "Bash"
    }

    fn aliases(&self) -> &[&'static str] {
        &["sh"]
    }

    fn supports_sessions(&self) -> bool {
        true
    }

    fn validate(&self) -> Result<()> {
        let mut cmd = self.run_command();
        cmd.arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        cmd.status()
            .with_context(|| format!("failed to invoke {}", self.binary().display()))?
            .success()
            .then_some(())
            .ok_or_else(|| anyhow::anyhow!("{} is not executable", self.binary().display()))
    }

    fn execute(&self, payload: &ExecutionPayload) -> Result<ExecutionOutcome> {
        let start = Instant::now();
        let output = match payload {
            ExecutionPayload::Inline { code } => {
                let mut cmd = self.run_command();
                cmd.arg("-c").arg(code);
                cmd.stdin(Stdio::inherit());
                cmd.output()
            }
            ExecutionPayload::File { path } => {
                let mut cmd = self.run_command();
                cmd.arg(path);
                cmd.stdin(Stdio::inherit());
                cmd.output()
            }
            ExecutionPayload::Stdin { code } => {
                let mut cmd = self.run_command();
                cmd.stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());
                let mut child = cmd.spawn().with_context(|| {
                    format!(
                        "failed to start {} for stdin execution",
                        self.binary().display()
                    )
                })?;
                if let Some(mut stdin) = child.stdin.take() {
                    stdin.write_all(code.as_bytes())?;
                    if !code.ends_with('\n') {
                        stdin.write_all(b"\n")?;
                    }
                    stdin.flush()?;
                }
                child.wait_with_output()
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
        Ok(Box::new(BashSession::new(self.executable.clone())?))
    }
}

fn resolve_bash_binary() -> PathBuf {
    let candidates = ["bash", "sh"];
    for name in candidates {
        if let Ok(path) = which::which(name) {
            return path;
        }
    }
    PathBuf::from("/bin/bash")
}

struct BashSession {
    executable: PathBuf,
    dir: TempDir,
    script_path: PathBuf,
    statements: Vec<String>,
    previous_stdout: String,
    previous_stderr: String,
}

impl BashSession {
    fn new(executable: PathBuf) -> Result<Self> {
        let dir = Builder::new()
            .prefix("run-bash-repl")
            .tempdir()
            .context("failed to create temporary directory for bash repl")?;
        let script_path = dir.path().join("session.sh");
        fs::write(&script_path, "#!/usr/bin/env bash\nset -e\n")
            .with_context(|| format!("failed to initialize {}", script_path.display()))?;

        Ok(Self {
            executable,
            dir,
            script_path,
            statements: Vec::new(),
            previous_stdout: String::new(),
            previous_stderr: String::new(),
        })
    }

    fn render_script(&self) -> String {
        let mut script = String::from("#!/usr/bin/env bash\nset -e\n");
        for stmt in &self.statements {
            script.push_str(stmt);
            if !stmt.ends_with('\n') {
                script.push('\n');
            }
        }
        script
    }

    fn write_script(&self, contents: &str) -> Result<()> {
        fs::write(&self.script_path, contents).with_context(|| {
            format!(
                "failed to write generated Bash REPL script to {}",
                self.script_path.display()
            )
        })
    }

    fn run_current(&mut self, start: Instant) -> Result<(ExecutionOutcome, bool)> {
        let script = self.render_script();
        self.write_script(&script)?;

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
            language: "bash".to_string(),
            exit_code: output.status.code(),
            stdout: stdout_delta,
            stderr: stderr_delta,
            duration: start.elapsed(),
        };

        Ok((outcome, success))
    }

    fn run_script(&self) -> Result<std::process::Output> {
        let mut cmd = Command::new(&self.executable);
        cmd.arg(&self.script_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(self.dir.path());
        cmd.output().with_context(|| {
            format!(
                "failed to execute bash session script {} with {}",
                self.script_path.display(),
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
            let script = self.render_script();
            self.write_script(&script)?;
        }
        Ok(outcome)
    }

    fn reset_state(&mut self) -> Result<()> {
        self.statements.clear();
        self.previous_stdout.clear();
        self.previous_stderr.clear();
        let script = self.render_script();
        self.write_script(&script)
    }
}

impl LanguageSession for BashSession {
    fn language_id(&self) -> &str {
        "bash"
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
                    "Bash commands:\n  :reset - clear session state\n  :help  - show this message\n"
                        .to_string(),
                stderr: String::new(),
                duration: Duration::default(),
            });
        }

        let snippet = ensure_trailing_newline(code);
        self.run_snippet(snippet)
    }

    fn shutdown(&mut self) -> Result<()> {
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
