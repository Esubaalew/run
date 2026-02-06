use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tempfile::Builder;

use super::{ExecutionOutcome, ExecutionPayload, LanguageEngine, LanguageSession, hash_source};

pub struct JavaEngine {
    compiler: Option<PathBuf>,
    runtime: Option<PathBuf>,
    jshell: Option<PathBuf>,
}

impl Default for JavaEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl JavaEngine {
    pub fn new() -> Self {
        Self {
            compiler: resolve_javac_binary(),
            runtime: resolve_java_binary(),
            jshell: resolve_jshell_binary(),
        }
    }

    fn ensure_compiler(&self) -> Result<&Path> {
        self.compiler.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "Java support requires the `javac` compiler. Install the JDK from https://adoptium.net/ or your vendor of choice and ensure it is on your PATH."
            )
        })
    }

    fn ensure_runtime(&self) -> Result<&Path> {
        self.runtime.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "Java support requires the `java` runtime. Install the JDK from https://adoptium.net/ or your vendor of choice and ensure it is on your PATH."
            )
        })
    }

    fn ensure_jshell(&self) -> Result<&Path> {
        self.jshell.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "Interactive Java REPL requires `jshell`. Install a full JDK and ensure `jshell` is on your PATH."
            )
        })
    }

    fn write_inline_source(&self, code: &str, dir: &Path) -> Result<(PathBuf, String)> {
        let source_path = dir.join("Main.java");
        let wrapped = wrap_inline_java(code);
        std::fs::write(&source_path, wrapped).with_context(|| {
            format!(
                "failed to write temporary Java source to {}",
                source_path.display()
            )
        })?;
        Ok((source_path, "Main".to_string()))
    }

    fn write_from_stdin(&self, code: &str, dir: &Path) -> Result<(PathBuf, String)> {
        self.write_inline_source(code, dir)
    }

    fn copy_source(&self, original: &Path, dir: &Path) -> Result<(PathBuf, String)> {
        let file_name = original
            .file_name()
            .map(|f| f.to_owned())
            .ok_or_else(|| anyhow::anyhow!("invalid Java source path"))?;
        let target = dir.join(&file_name);
        std::fs::copy(original, &target).with_context(|| {
            format!(
                "failed to copy Java source from {} to {}",
                original.display(),
                target.display()
            )
        })?;
        let class_name = original
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| anyhow::anyhow!("unable to determine Java class name"))?
            .to_string();
        Ok((target, class_name))
    }

    fn compile(&self, source: &Path, output_dir: &Path) -> Result<std::process::Output> {
        let compiler = self.ensure_compiler()?;
        let mut cmd = Command::new(compiler);
        cmd.arg("-d")
            .arg(output_dir)
            .arg(source)
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

    fn run(&self, class_dir: &Path, class_name: &str) -> Result<std::process::Output> {
        let runtime = self.ensure_runtime()?;
        let mut cmd = Command::new(runtime);
        cmd.arg("-cp")
            .arg(class_dir)
            .arg(class_name)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd.stdin(Stdio::inherit());
        cmd.output().with_context(|| {
            format!(
                "failed to execute {} for class {} with classpath {}",
                runtime.display(),
                class_name,
                class_dir.display()
            )
        })
    }
}

impl LanguageEngine for JavaEngine {
    fn id(&self) -> &'static str {
        "java"
    }

    fn display_name(&self) -> &'static str {
        "Java"
    }

    fn aliases(&self) -> &[&'static str] {
        &[]
    }

    fn supports_sessions(&self) -> bool {
        self.jshell.is_some()
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

        let runtime = self.ensure_runtime()?;
        let mut runtime_check = Command::new(runtime);
        runtime_check
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        runtime_check
            .status()
            .with_context(|| format!("failed to invoke {}", runtime.display()))?
            .success()
            .then_some(())
            .ok_or_else(|| anyhow::anyhow!("{} is not executable", runtime.display()))?;

        if let Some(jshell) = self.jshell.as_ref() {
            let mut jshell_check = Command::new(jshell);
            jshell_check
                .arg("--version")
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            jshell_check
                .status()
                .with_context(|| format!("failed to invoke {}", jshell.display()))?
                .success()
                .then_some(())
                .ok_or_else(|| anyhow::anyhow!("{} is not executable", jshell.display()))?;
        }

        Ok(())
    }

    fn execute(&self, payload: &ExecutionPayload) -> Result<ExecutionOutcome> {
        // Check class file cache for inline/stdin payloads
        if let Some(code) = match payload {
            ExecutionPayload::Inline { code } | ExecutionPayload::Stdin { code } => {
                Some(code.as_str())
            }
            _ => None,
        } {
            let wrapped = wrap_inline_java(code);
            let src_hash = hash_source(&wrapped);
            let cache_dir = std::env::temp_dir()
                .join("run-compile-cache")
                .join(format!("java-{:016x}", src_hash));
            let class_file = cache_dir.join("Main.class");
            if class_file.exists() {
                let start = Instant::now();
                if let Ok(output) = self.run(&cache_dir, "Main") {
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
            .prefix("run-java")
            .tempdir()
            .context("failed to create temporary directory for java build")?;
        let dir_path = temp_dir.path();

        let (source_path, class_name) = match payload {
            ExecutionPayload::Inline { code } => self.write_inline_source(code, dir_path)?,
            ExecutionPayload::Stdin { code } => self.write_from_stdin(code, dir_path)?,
            ExecutionPayload::File { path } => self.copy_source(path, dir_path)?,
        };

        let start = Instant::now();

        let compile_output = self.compile(&source_path, dir_path)?;
        if !compile_output.status.success() {
            return Ok(ExecutionOutcome {
                language: self.id().to_string(),
                exit_code: compile_output.status.code(),
                stdout: String::from_utf8_lossy(&compile_output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&compile_output.stderr).into_owned(),
                duration: start.elapsed(),
            });
        }

        // Cache compiled class files for inline/stdin
        if let Some(code) = match payload {
            ExecutionPayload::Inline { code } | ExecutionPayload::Stdin { code } => {
                Some(code.as_str())
            }
            _ => None,
        } {
            let wrapped = wrap_inline_java(code);
            let src_hash = hash_source(&wrapped);
            let cache_dir = std::env::temp_dir()
                .join("run-compile-cache")
                .join(format!("java-{:016x}", src_hash));
            let _ = std::fs::create_dir_all(&cache_dir);
            // Copy all .class files
            if let Ok(entries) = std::fs::read_dir(dir_path) {
                for entry in entries.flatten() {
                    if entry.path().extension().and_then(|e| e.to_str()) == Some("class") {
                        let _ = std::fs::copy(entry.path(), cache_dir.join(entry.file_name()));
                    }
                }
            }
        }

        let run_output = self.run(dir_path, &class_name)?;
        Ok(ExecutionOutcome {
            language: self.id().to_string(),
            exit_code: run_output.status.code(),
            stdout: String::from_utf8_lossy(&run_output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&run_output.stderr).into_owned(),
            duration: start.elapsed(),
        })
    }

    fn start_session(&self) -> Result<Box<dyn LanguageSession>> {
        let jshell = self.ensure_jshell()?;
        let mut cmd = Command::new(jshell);
        cmd.arg("--execution=local")
            .arg("--feedback=concise")
            .arg("--no-startup")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .with_context(|| format!("failed to start {} REPL", jshell.display()))?;

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

        let mut session = JavaSession {
            child,
            stdout: BufReader::new(stdout),
            stderr: stderr_buffer,
            closed: false,
        };

        session.discard_prompt()?;

        Ok(Box::new(session))
    }
}

fn resolve_javac_binary() -> Option<PathBuf> {
    which::which("javac").ok()
}

fn resolve_java_binary() -> Option<PathBuf> {
    which::which("java").ok()
}

fn resolve_jshell_binary() -> Option<PathBuf> {
    which::which("jshell").ok()
}

fn wrap_inline_java(body: &str) -> String {
    if body.contains("class ") {
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

    result.push_str(
        "public class Main {\n    public static void main(String[] args) throws Exception {\n",
    );
    for line in rest_lines {
        if line.trim().is_empty() {
            result.push_str("        \n");
        } else {
            result.push_str("        ");
            result.push_str(line);
            result.push('\n');
        }
    }
    result.push_str("    }\n}\n");
    result
}

struct JavaSession {
    child: std::process::Child,
    stdout: BufReader<std::process::ChildStdout>,
    stderr: Arc<Mutex<String>>,
    closed: bool,
}

impl JavaSession {
    fn write_code(&mut self, code: &str) -> Result<()> {
        if self.closed {
            anyhow::bail!("jshell session has already exited; start a new session with :reset");
        }
        let stdin = self
            .child
            .stdin
            .as_mut()
            .context("jshell session stdin closed")?;
        stdin.write_all(code.as_bytes())?;
        if !code.ends_with('\n') {
            stdin.write_all(b"\n")?;
        }
        stdin.flush()?;
        Ok(())
    }

    fn read_until_prompt(&mut self) -> Result<String> {
        const PROMPT: &[u8] = b"jshell> ";
        let mut buffer = Vec::new();
        loop {
            let mut byte = [0u8; 1];
            let read = self.stdout.read(&mut byte)?;
            if read == 0 {
                break;
            }
            buffer.extend_from_slice(&byte[..read]);
            if buffer.ends_with(PROMPT) {
                break;
            }
        }

        if buffer.ends_with(PROMPT) {
            buffer.truncate(buffer.len() - PROMPT.len());
        }

        let mut text = String::from_utf8_lossy(&buffer).into_owned();
        text = text.replace("\r\n", "\n");
        text = text.replace('\r', "");
        Ok(strip_feedback(text))
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

impl LanguageSession for JavaSession {
    fn language_id(&self) -> &str {
        "java"
    }

    fn eval(&mut self, code: &str) -> Result<ExecutionOutcome> {
        if self.closed {
            return Ok(ExecutionOutcome {
                language: self.language_id().to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: "jshell session already exited. Use :reset to start a new session.\n"
                    .to_string(),
                duration: Duration::default(),
            });
        }

        let trimmed = code.trim();
        let exit_requested = matches!(trimmed, "/exit" | "/exit;" | ":exit");
        let start = Instant::now();
        self.write_code(code)?;
        let stdout = match self.read_until_prompt() {
            Ok(output) => output,
            Err(_) if exit_requested => String::new(),
            Err(err) => return Err(err),
        };
        let stderr = self.take_stderr();

        if exit_requested {
            self.closed = true;
            let _ = self.child.stdin.take();
            let _ = self.child.wait();
        }

        Ok(ExecutionOutcome {
            language: self.language_id().to_string(),
            exit_code: None,
            stdout,
            stderr,
            duration: start.elapsed(),
        })
    }

    fn shutdown(&mut self) -> Result<()> {
        if !self.closed
            && let Some(mut stdin) = self.child.stdin.take()
        {
            let _ = stdin.write_all(b"/exit\n");
            let _ = stdin.flush();
        }
        let _ = self.child.wait();
        self.closed = true;
        Ok(())
    }
}

fn strip_feedback(text: String) -> String {
    let mut lines = Vec::new();
    for line in text.lines() {
        if let Some(stripped) = line.strip_prefix("|  ") {
            lines.push(stripped.to_string());
        } else if let Some(stripped) = line.strip_prefix("| ") {
            lines.push(stripped.to_string());
        } else if line.starts_with("|=") {
            lines.push(line.trim_start_matches('|').trim().to_string());
        } else if !line.trim().is_empty() {
            lines.push(line.to_string());
        }
    }
    lines.join("\n")
}
