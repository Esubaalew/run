use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use anyhow::{Context, Result};
use std::sync::{Arc, Mutex};
use std::thread;

use super::{
    ExecutionOutcome, ExecutionPayload, LanguageEngine, LanguageSession, execution_timeout,
    wait_with_timeout,
};

pub struct JavascriptEngine {
    executable: PathBuf,
}

impl Default for JavascriptEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl JavascriptEngine {
    pub fn new() -> Self {
        let executable = resolve_node_binary();
        Self { executable }
    }

    fn binary(&self) -> &Path {
        &self.executable
    }

    fn run_command(&self) -> Command {
        Command::new(self.binary())
    }
}

impl LanguageEngine for JavascriptEngine {
    fn id(&self) -> &'static str {
        "javascript"
    }

    fn display_name(&self) -> &'static str {
        "JavaScript"
    }

    fn aliases(&self) -> &[&'static str] {
        &["js", "node", "nodejs"]
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
        let timeout = execution_timeout();
        let output = match payload {
            ExecutionPayload::Inline { code } => {
                let mut cmd = self.run_command();
                cmd.arg("-e")
                    .arg(code)
                    .stdin(Stdio::inherit())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());
                let child = cmd
                    .spawn()
                    .with_context(|| format!("failed to start {}", self.binary().display()))?;
                wait_with_timeout(child, timeout)?
            }
            ExecutionPayload::File { path } => {
                let mut cmd = self.run_command();
                cmd.arg(path)
                    .stdin(Stdio::inherit())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());
                let child = cmd
                    .spawn()
                    .with_context(|| format!("failed to start {}", self.binary().display()))?;
                wait_with_timeout(child, timeout)?
            }
            ExecutionPayload::Stdin { code } => {
                let mut cmd = self.run_command();
                cmd.arg("-")
                    .stdin(Stdio::piped())
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
                wait_with_timeout(child, timeout)?
            }
        };

        Ok(ExecutionOutcome {
            language: self.id().to_string(),
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            duration: start.elapsed(),
        })
    }

    fn start_session(&self) -> Result<Box<dyn LanguageSession>> {
        let mut cmd = self.run_command();
        cmd.arg("--interactive")
            .arg("--no-warnings")
            .arg("--experimental-repl-await")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .with_context(|| format!("failed to start {} REPL", self.binary().display()))?;

        let stdout = child.stdout.take().context("missing stdout handle")?;
        let stderr = child.stderr.take().context("missing stderr handle")?;

        let stderr_buffer = Arc::new(Mutex::new(String::new()));
        let stderr_collector = stderr_buffer.clone();
        thread::spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut buf = String::new();
            loop {
                buf.clear();
                match reader.read_line(&mut buf) {
                    Ok(0) => break,
                    Ok(_) => {
                        let Ok(mut lock) = stderr_collector.lock() else {
                            break;
                        };
                        lock.push_str(&buf);
                    }
                    Err(_) => break,
                }
            }
        });

        let mut session = JavascriptSession {
            child,
            stdout: BufReader::new(stdout),
            stderr: stderr_buffer,
        };

        session.discard_prompt()?;

        Ok(Box::new(session))
    }
}

fn resolve_node_binary() -> PathBuf {
    let candidates = ["node", "nodejs"];
    for name in candidates {
        if let Ok(path) = which::which(name) {
            return path;
        }
    }
    PathBuf::from("node")
}

struct JavascriptSession {
    child: std::process::Child,
    stdout: BufReader<std::process::ChildStdout>,
    stderr: Arc<Mutex<String>>,
}

impl JavascriptSession {
    fn write_code(&mut self, code: &str) -> Result<()> {
        let stdin = self
            .child
            .stdin
            .as_mut()
            .context("javascript session stdin closed")?;
        stdin.write_all(code.as_bytes())?;
        if !code.ends_with('\n') {
            stdin.write_all(b"\n")?;
        }
        stdin.flush()?;
        Ok(())
    }

    fn read_until_prompt(&mut self) -> Result<String> {
        const PROMPT: &[u8] = b"> ";
        const CONT_PROMPT: &[u8] = b"... ";
        let mut buffer = Vec::new();
        loop {
            let mut byte = [0u8; 1];
            let read = self.stdout.read(&mut byte)?;
            if read == 0 {
                break;
            }
            buffer.extend_from_slice(&byte[..read]);
            if buffer.ends_with(PROMPT) && !buffer.ends_with(CONT_PROMPT) {
                break;
            }
        }

        while buffer.ends_with(PROMPT) {
            buffer.truncate(buffer.len() - PROMPT.len());
        }

        let mut text = String::from_utf8_lossy(&buffer).into_owned();
        text = text.replace("\r\n", "\n");
        text = text.replace('\r', "");
        text = trim_continuation_prompt(text, "... ");
        Ok(text.trim_start_matches('\n').to_string())
    }

    fn take_stderr(&self) -> String {
        let Ok(mut lock) = self.stderr.lock() else {
            return String::new();
        };
        if lock.is_empty() {
            String::new()
        } else {
            let mut output = String::new();
            std::mem::swap(&mut output, &mut *lock);
            output
        }
    }

    fn discard_prompt(&mut self) -> Result<()> {
        let _ = self.read_until_prompt()?;
        let _ = self.take_stderr();
        Ok(())
    }
}

impl LanguageSession for JavascriptSession {
    fn language_id(&self) -> &str {
        "javascript"
    }

    fn eval(&mut self, code: &str) -> Result<ExecutionOutcome> {
        // Node.js REPL natively stores the last expression result in `_`.
        let start = Instant::now();
        self.write_code(code)?;
        let stdout = self.read_until_prompt()?;
        let stderr = self.take_stderr();
        Ok(ExecutionOutcome {
            language: self.language_id().to_string(),
            exit_code: None,
            stdout,
            stderr,
            duration: start.elapsed(),
        })
    }

    fn shutdown(&mut self) -> Result<()> {
        if let Some(mut stdin) = self.child.stdin.take() {
            let _ = stdin.write_all(b".exit\n");
            let _ = stdin.flush();
        }
        let _ = self.child.wait();
        Ok(())
    }
}

fn trim_continuation_prompt(mut text: String, prompt: &str) -> String {
    if text.contains(prompt) {
        text = text
            .lines()
            .map(|line| line.strip_prefix(prompt).unwrap_or(line))
            .collect::<Vec<_>>()
            .join("\n");
    }
    text
}
