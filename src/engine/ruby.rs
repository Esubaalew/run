use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use anyhow::{Context, Result};
use std::sync::{Arc, Mutex};
use std::thread;

use super::{ExecutionOutcome, ExecutionPayload, LanguageEngine, LanguageSession};

pub struct RubyEngine {
    executable: PathBuf,
    irb: Option<PathBuf>,
}

impl Default for RubyEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl RubyEngine {
    pub fn new() -> Self {
        let executable = resolve_ruby_binary();
        let irb = resolve_irb_binary();
        Self { executable, irb }
    }

    fn binary(&self) -> &Path {
        &self.executable
    }

    fn run_command(&self) -> Command {
        Command::new(self.binary())
    }

    fn ensure_irb(&self) -> Result<&Path> {
        self.irb.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "Interactive Ruby REPL requires the `irb` executable. Install it with your Ruby distribution and ensure it is on your PATH."
            )
        })
    }
}

impl LanguageEngine for RubyEngine {
    fn id(&self) -> &'static str {
        "ruby"
    }

    fn display_name(&self) -> &'static str {
        "Ruby"
    }

    fn aliases(&self) -> &[&'static str] {
        &["rb"]
    }

    fn supports_sessions(&self) -> bool {
        self.irb.is_some()
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
                cmd.arg("-e").arg(code);
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
        let irb = self.ensure_irb()?;
        let mut cmd = Command::new(irb);
        cmd.arg("--simple-prompt")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .with_context(|| format!("failed to start {} REPL", irb.display()))?;

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
                        let mut lock = stderr_collector.lock().expect("stderr collector poisoned");
                        lock.push_str(&buf);
                    }
                    Err(_) => break,
                }
            }
        });

        let mut session = RubySession {
            child,
            stdout: BufReader::new(stdout),
            stderr: stderr_buffer,
        };

        session.discard_prompt()?;

        Ok(Box::new(session))
    }
}

fn resolve_ruby_binary() -> PathBuf {
    let candidates = ["ruby"];
    for name in candidates {
        if let Ok(path) = which::which(name) {
            return path;
        }
    }
    PathBuf::from("ruby")
}

fn resolve_irb_binary() -> Option<PathBuf> {
    which::which("irb").ok()
}

struct RubySession {
    child: std::process::Child,
    stdout: BufReader<std::process::ChildStdout>,
    stderr: Arc<Mutex<String>>,
}

impl RubySession {
    fn write_code(&mut self, code: &str) -> Result<()> {
        let stdin = self
            .child
            .stdin
            .as_mut()
            .context("ruby session stdin closed")?;
        stdin.write_all(code.as_bytes())?;
        if !code.ends_with('\n') {
            stdin.write_all(b"\n")?;
        }
        stdin.flush()?;
        Ok(())
    }

    fn read_until_prompt(&mut self) -> Result<String> {
        const PROMPTS: &[&[u8]] = &[b">> ", b"?> ", b"%l> ", b"*> "];
        let mut buffer = Vec::new();
        loop {
            let mut byte = [0u8; 1];
            let read = self.stdout.read(&mut byte)?;
            if read == 0 {
                break;
            }
            buffer.extend_from_slice(&byte[..read]);
            if PROMPTS.iter().any(|prompt| buffer.ends_with(prompt)) {
                break;
            }
        }

        if let Some(prompt) = PROMPTS.iter().find(|prompt| buffer.ends_with(prompt)) {
            buffer.truncate(buffer.len() - prompt.len());
        }

        let mut text = String::from_utf8_lossy(&buffer).into_owned();
        text = text.replace("\r\n", "\n");
        text = text.replace('\r', "");
        Ok(strip_ruby_result(text))
    }

    fn take_stderr(&self) -> String {
        let mut lock = self.stderr.lock().expect("stderr lock poisoned");
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

impl LanguageSession for RubySession {
    fn language_id(&self) -> &str {
        "ruby"
    }

    fn eval(&mut self, code: &str) -> Result<ExecutionOutcome> {
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
            let _ = stdin.write_all(b"exit\n");
            let _ = stdin.flush();
        }
        let _ = self.child.wait();
        Ok(())
    }
}

fn strip_ruby_result(text: String) -> String {
    let mut lines = Vec::new();
    for line in text.lines() {
        if let Some(stripped) = line.strip_prefix("=> ") {
            lines.push(stripped.to_string());
        } else if !line.trim().is_empty() {
            lines.push(line.to_string());
        }
    }
    lines.join("\n")
}
