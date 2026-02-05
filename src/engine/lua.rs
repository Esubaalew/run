use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tempfile::{Builder, TempDir};

use super::{ExecutionOutcome, ExecutionPayload, LanguageEngine, LanguageSession};

pub struct LuaEngine {
    interpreter: Option<PathBuf>,
}

impl LuaEngine {
    pub fn new() -> Self {
        Self {
            interpreter: resolve_lua_binary(),
        }
    }

    fn ensure_interpreter(&self) -> Result<&Path> {
        self.interpreter.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "Lua support requires the `lua` executable. Install it from https://www.lua.org/download.html and ensure it is on your PATH." 
            )
        })
    }

    fn write_temp_script(&self, code: &str) -> Result<(tempfile::TempDir, PathBuf)> {
        let dir = Builder::new()
            .prefix("run-lua")
            .tempdir()
            .context("failed to create temporary directory for lua source")?;
        let path = dir.path().join("snippet.lua");
        let mut contents = code.to_string();
        if !contents.ends_with('\n') {
            contents.push('\n');
        }
        std::fs::write(&path, contents).with_context(|| {
            format!("failed to write temporary Lua source to {}", path.display())
        })?;
        Ok((dir, path))
    }

    fn execute_script(&self, script: &Path) -> Result<std::process::Output> {
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

impl LanguageEngine for LuaEngine {
    fn id(&self) -> &'static str {
        "lua"
    }

    fn display_name(&self) -> &'static str {
        "Lua"
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

        let output = self.execute_script(&script_path)?;

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
        let session = LuaSession::new(interpreter)?;
        Ok(Box::new(session))
    }
}

fn resolve_lua_binary() -> Option<PathBuf> {
    which::which("lua").ok()
}

const SESSION_MAIN_FILE: &str = "session.lua";

struct LuaSession {
    interpreter: PathBuf,
    workspace: TempDir,
    statements: Vec<String>,
    last_stdout: String,
    last_stderr: String,
}

impl LuaSession {
    fn new(interpreter: PathBuf) -> Result<Self> {
        let workspace = TempDir::new().context("failed to create Lua session workspace")?;
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
        "lua"
    }

    fn source_path(&self) -> PathBuf {
        self.workspace.path().join(SESSION_MAIN_FILE)
    }

    fn persist_source(&self) -> Result<()> {
        let path = self.source_path();
        let mut source = String::new();
        if self.statements.is_empty() {
            source.push_str("-- session body\n");
        } else {
            for stmt in &self.statements {
                source.push_str(stmt);
                if !stmt.ends_with('\n') {
                    source.push('\n');
                }
            }
        }
        fs::write(&path, source)
            .with_context(|| format!("failed to write Lua session source at {}", path.display()))
    }

    fn run_program(&self) -> Result<std::process::Output> {
        let mut cmd = Command::new(&self.interpreter);
        cmd.arg(SESSION_MAIN_FILE)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(self.workspace.path());
        cmd.output().with_context(|| {
            format!(
                "failed to execute {} for Lua session",
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

fn looks_like_expression_snippet(code: &str) -> bool {
    if code.is_empty() || code.contains('\n') {
        return false;
    }

    let trimmed = code.trim();
    if trimmed.is_empty() {
        return false;
    }

    let lower = trimmed.to_ascii_lowercase();
    const CONTROL_KEYWORDS: &[&str] = &[
        "local", "function", "for", "while", "repeat", "if", "do", "return", "break", "goto", "end",
    ];

    for kw in CONTROL_KEYWORDS {
        if lower == *kw
            || lower.starts_with(&format!("{} ", kw))
            || lower.starts_with(&format!("{}(", kw))
            || lower.starts_with(&format!("{}\t", kw))
        {
            return false;
        }
    }

    if lower.starts_with("--") {
        return false;
    }

    if has_assignment_operator(trimmed) {
        return false;
    }

    true
}

fn has_assignment_operator(code: &str) -> bool {
    let bytes = code.as_bytes();
    for (i, byte) in bytes.iter().enumerate() {
        if *byte == b'=' {
            let prev = if i > 0 { bytes[i - 1] } else { b'\0' };
            let next = if i + 1 < bytes.len() {
                bytes[i + 1]
            } else {
                b'\0'
            };
            let part_of_comparison = matches!(prev, b'=' | b'<' | b'>' | b'~') || next == b'=';
            if !part_of_comparison {
                return true;
            }
        }
    }
    false
}

fn wrap_expression_snippet(code: &str) -> String {
    let trimmed = code.trim();
    format!(
        "do\n    local __run_pack = table.pack(({expr}))\n    local __run_n = __run_pack.n or #__run_pack\n    if __run_n > 0 then\n        for __run_i = 1, __run_n do\n            if __run_i > 1 then io.write(\"\\t\") end\n            local __run_val = __run_pack[__run_i]\n            if __run_val == nil then\n                io.write(\"nil\")\n            else\n                io.write(tostring(__run_val))\n            end\n        end\n        io.write(\"\\n\")\n    end\nend\n",
        expr = trimmed
    )
}
impl LanguageSession for LuaSession {
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

        if trimmed.eq_ignore_ascii_case(":help") {
            return Ok(ExecutionOutcome {
                language: self.language_id().to_string(),
                exit_code: None,
                stdout:
                    "Lua commands:\n  :reset - clear session state\n  :help  - show this message\n"
                        .to_string(),
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

        let (effective_code, force_expression) = if trimmed.starts_with('=') {
            (trimmed[1..].trim(), true)
        } else {
            (trimmed, false)
        };

        let is_expression = force_expression || looks_like_expression_snippet(effective_code);
        let statement = if is_expression {
            wrap_expression_snippet(effective_code)
        } else {
            format!("{}\n", code.trim_end_matches(|c| c == '\r' || c == '\n'))
        };

        let previous_stdout = self.last_stdout.clone();
        let previous_stderr = self.last_stderr.clone();

        self.statements.push(statement);
        self.persist_source()?;

        let start = Instant::now();
        let output = self.run_program()?;
        let stdout_full = LuaSession::normalize_output(&output.stdout);
        let stderr_full = LuaSession::normalize_output(&output.stderr);
        let stdout = LuaSession::diff_outputs(&self.last_stdout, &stdout_full);
        let stderr = LuaSession::diff_outputs(&self.last_stderr, &stderr_full);
        let duration = start.elapsed();

        if output.status.success() {
            if is_expression {
                self.statements.pop();
                self.persist_source()?;
                self.last_stdout = previous_stdout;
                self.last_stderr = previous_stderr;
            } else {
                self.last_stdout = stdout_full;
                self.last_stderr = stderr_full;
            }
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
            self.last_stdout = previous_stdout;
            self.last_stderr = previous_stderr;
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

#[cfg(test)]
mod tests {
    use super::{looks_like_expression_snippet, wrap_expression_snippet, LuaSession};

    #[test]
    fn diff_outputs_appends_only_suffix() {
        let previous = "a\nb\n";
        let current = "a\nb\nc\n";
        assert_eq!(LuaSession::diff_outputs(previous, current), "c\n");

        let previous = "a\n";
        let current = "x\na\n";
        assert_eq!(LuaSession::diff_outputs(previous, current), "x\na\n");
    }

    #[test]
    fn detects_simple_expression() {
        assert!(looks_like_expression_snippet("a"));
        assert!(looks_like_expression_snippet("foo(bar)"));
        assert!(!looks_like_expression_snippet("local a = 1"));
        assert!(!looks_like_expression_snippet("a = 1"));
    }

    #[test]
    fn wraps_expression_with_print_block() {
        let wrapped = wrap_expression_snippet("a");
        assert!(wrapped.contains("table.pack((a))"));
        assert!(wrapped.contains("io.write(\"\\n\")"));
    }
}
