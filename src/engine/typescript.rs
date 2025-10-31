use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use tempfile::{NamedTempFile, TempDir};

use super::{ExecutionOutcome, ExecutionPayload, LanguageEngine, LanguageSession};

pub struct TypeScriptEngine {
    executable: PathBuf,
}

impl TypeScriptEngine {
    pub fn new() -> Self {
        let executable = resolve_deno_binary();
        Self { executable }
    }

    fn binary(&self) -> &Path {
        &self.executable
    }

    fn run_command(&self) -> Command {
        Command::new(self.binary())
    }
}

impl LanguageEngine for TypeScriptEngine {
    fn id(&self) -> &'static str {
        "typescript"
    }

    fn display_name(&self) -> &'static str {
        "TypeScript"
    }

    fn aliases(&self) -> &[&'static str] {
        &["ts", "deno"]
    }

    fn supports_sessions(&self) -> bool {
        true
    }

    fn validate(&self) -> Result<()> {
        let mut cmd = self.run_command();
        cmd.arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let status = handle_deno_io(
            cmd.status(),
            self.binary(),
            "invoke Deno to check its version",
        )?;

        if status.success() {
            Ok(())
        } else {
            bail!("{} is not executable", self.binary().display());
        }
    }

    fn execute(&self, payload: &ExecutionPayload) -> Result<ExecutionOutcome> {
        let start = Instant::now();
        let output = match payload {
            ExecutionPayload::Inline { code } => {
                let mut script =
                    NamedTempFile::new().context("failed to create temporary TypeScript file")?;
                script.write_all(code.as_bytes())?;
                if !code.ends_with('\n') {
                    script.write_all(b"\n")?;
                }
                script.flush()?;

                let mut cmd = self.run_command();
                cmd.arg("run")
                    .args(["--quiet", "--no-check", "--ext", "ts"])
                    .arg(script.path())
                    .env("NO_COLOR", "1");
                cmd.stdin(Stdio::inherit());
                handle_deno_io(cmd.output(), self.binary(), "run Deno for inline execution")?
            }
            ExecutionPayload::File { path } => {
                let mut cmd = self.run_command();
                cmd.arg("run")
                    .args(["--quiet", "--no-check", "--ext", "ts"])
                    .arg(path)
                    .env("NO_COLOR", "1");
                cmd.stdin(Stdio::inherit());
                handle_deno_io(cmd.output(), self.binary(), "run Deno for file execution")?
            }
            ExecutionPayload::Stdin { code } => {
                let mut cmd = self.run_command();
                cmd.arg("run")
                    .args(["--quiet", "--no-check", "--ext", "ts", "-"])
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .env("NO_COLOR", "1");

                let mut child =
                    handle_deno_io(cmd.spawn(), self.binary(), "start Deno for stdin execution")?;

                if let Some(mut stdin) = child.stdin.take() {
                    stdin.write_all(code.as_bytes())?;
                    if !code.ends_with('\n') {
                        stdin.write_all(b"\n")?;
                    }
                    stdin.flush()?;
                }

                handle_deno_io(
                    child.wait_with_output(),
                    self.binary(),
                    "read output from Deno stdin execution",
                )?
            }
        };

        Ok(ExecutionOutcome {
            language: self.id().to_string(),
            exit_code: output.status.code(),
            stdout: strip_ansi_codes(&String::from_utf8_lossy(&output.stdout)).replace('\r', ""),
            stderr: strip_ansi_codes(&String::from_utf8_lossy(&output.stderr)).replace('\r', ""),
            duration: start.elapsed(),
        })
    }

    fn start_session(&self) -> Result<Box<dyn LanguageSession>> {
        self.validate()?;
        let session = TypeScriptSession::new(self.binary().to_path_buf())?;
        Ok(Box::new(session))
    }
}

fn resolve_deno_binary() -> PathBuf {
    which::which("deno").unwrap_or_else(|_| PathBuf::from("deno"))
}

fn strip_ansi_codes(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars();

    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            // Skip escape sequence
            if chars.next() == Some('[') {
                // Skip until we find a letter (end of escape sequence)
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            result.push(ch);
        }
    }

    result
}

fn handle_deno_io<T>(result: std::io::Result<T>, binary: &Path, action: &str) -> Result<T> {
    match result {
        Ok(value) => Ok(value),
        Err(err) if err.kind() == ErrorKind::NotFound => bail!(
            "failed to {} because '{}' was not found in PATH. Install Deno from https://deno.land/manual/getting_started/installation or ensure the binary is available on your PATH.",
            action,
            binary.display()
        ),
        Err(err) => {
            Err(err).with_context(|| format!("failed to {} using {}", action, binary.display()))
        }
    }
}

struct TypeScriptSession {
    deno_path: PathBuf,
    _workspace: TempDir,
    entrypoint: PathBuf,
    snippets: Vec<String>,
    last_stdout: String,
    last_stderr: String,
}

impl TypeScriptSession {
    fn new(deno_path: PathBuf) -> Result<Self> {
        let workspace = TempDir::new().context("failed to create TypeScript session workspace")?;
        let entrypoint = workspace.path().join("session.ts");
        let session = Self {
            deno_path,
            _workspace: workspace,
            entrypoint,
            snippets: Vec::new(),
            last_stdout: String::new(),
            last_stderr: String::new(),
        };
        session.persist_source()?;
        Ok(session)
    }

    fn language_id(&self) -> &str {
        "typescript"
    }

    fn persist_source(&self) -> Result<()> {
        let source = self.render_source();
        fs::write(&self.entrypoint, source)
            .with_context(|| "failed to write TypeScript session source".to_string())
    }

    fn render_source(&self) -> String {
        let mut source = String::from(
            r#"const __print = (value: unknown): void => {
    if (typeof value === "string") {
        console.log(value);
        return;
    }
    try {
        const serialized = JSON.stringify(value, null, 2);
        if (serialized !== undefined) {
            console.log(serialized);
            return;
        }
    } catch (_) {
        // ignore
    }
    console.log(String(value));
};

"#,
        );

        for snippet in &self.snippets {
            source.push_str(snippet);
            if !snippet.ends_with('\n') {
                source.push('\n');
            }
        }

        source
    }

    fn compile_and_run(&self) -> Result<std::process::Output> {
        let mut cmd = Command::new(&self.deno_path);
        cmd.arg("run")
            .args(["--quiet", "--no-check", "--ext", "ts"])
            .arg(&self.entrypoint)
            .env("NO_COLOR", "1");
        handle_deno_io(
            cmd.output(),
            &self.deno_path,
            "run Deno for the TypeScript session",
        )
    }

    fn normalize(text: &str) -> String {
        strip_ansi_codes(&text.replace("\r\n", "\n").replace('\r', ""))
    }

    fn diff_outputs(previous: &str, current: &str) -> String {
        if let Some(suffix) = current.strip_prefix(previous) {
            suffix.to_string()
        } else {
            current.to_string()
        }
    }

    fn run_snippet(&mut self, snippet: String) -> Result<(ExecutionOutcome, bool)> {
        let start = Instant::now();
        self.snippets.push(snippet);
        self.persist_source()?;
        let output = self.compile_and_run()?;

        let stdout_full = Self::normalize(&String::from_utf8_lossy(&output.stdout));
        let stderr_full = Self::normalize(&String::from_utf8_lossy(&output.stderr));

        let stdout = Self::diff_outputs(&self.last_stdout, &stdout_full);
        let stderr = Self::diff_outputs(&self.last_stderr, &stderr_full);
        let success = output.status.success();

        if success {
            self.last_stdout = stdout_full;
            self.last_stderr = stderr_full;
        } else {
            self.snippets.pop();
            self.persist_source()?;
        }

        let outcome = ExecutionOutcome {
            language: self.language_id().to_string(),
            exit_code: output.status.code(),
            stdout,
            stderr,
            duration: start.elapsed(),
        };

        Ok((outcome, success))
    }
}

impl LanguageSession for TypeScriptSession {
    fn language_id(&self) -> &str {
        TypeScriptSession::language_id(self)
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

        if should_treat_as_expression(trimmed) {
            let snippet = wrap_expression(trimmed);
            let (outcome, success) = self.run_snippet(snippet)?;
            if success {
                return Ok(outcome);
            }
        }

        let snippet = prepare_statement(code);
        let (outcome, _) = self.run_snippet(snippet)?;
        Ok(outcome)
    }

    fn shutdown(&mut self) -> Result<()> {
        Ok(())
    }
}

fn wrap_expression(code: &str) -> String {
    format!("__print(await ({}));\n", code)
}

fn prepare_statement(code: &str) -> String {
    let mut snippet = code.to_string();
    if !snippet.ends_with('\n') {
        snippet.push('\n');
    }
    snippet
}

fn should_treat_as_expression(code: &str) -> bool {
    let trimmed = code.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.contains('\n') {
        return false;
    }
    if trimmed.ends_with(';') || trimmed.contains(';') {
        return false;
    }
    const KEYWORDS: [&str; 11] = [
        "const ",
        "let ",
        "var ",
        "function ",
        "class ",
        "interface ",
        "type ",
        "import ",
        "export ",
        "if ",
        "while ",
    ];
    if KEYWORDS
        .iter()
        .any(|kw| trimmed.starts_with(kw) || trimmed.starts_with(&kw.to_ascii_uppercase()))
    {
        return false;
    }
    if trimmed.starts_with("return ") || trimmed.starts_with("throw ") {
        return false;
    }
    true
}
