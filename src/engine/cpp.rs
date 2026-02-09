use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tempfile::{Builder, TempDir};

use super::{
    ExecutionOutcome, ExecutionPayload, LanguageEngine, LanguageSession, cache_store, hash_source,
    run_version_command, try_cached_execution,
};

pub struct CppEngine {
    compiler: Option<PathBuf>,
}

impl Default for CppEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl CppEngine {
    pub fn new() -> Self {
        Self {
            compiler: resolve_cpp_compiler(),
        }
    }

    fn ensure_compiler(&self) -> Result<&Path> {
        self.compiler.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "C++ support requires a C++ compiler such as `c++`, `clang++`, or `g++`. Install one and ensure it is on your PATH."
            )
        })
    }

    fn write_source(&self, code: &str, dir: &Path) -> Result<PathBuf> {
        let source_path = dir.join("main.cpp");
        std::fs::write(&source_path, code).with_context(|| {
            format!(
                "failed to write temporary C++ source to {}",
                source_path.display()
            )
        })?;
        Ok(source_path)
    }

    fn copy_source(&self, original: &Path, dir: &Path) -> Result<PathBuf> {
        let target = dir.join("main.cpp");
        std::fs::copy(original, &target).with_context(|| {
            format!(
                "failed to copy C++ source from {} to {}",
                original.display(),
                target.display()
            )
        })?;
        Ok(target)
    }

    fn compile(&self, source: &Path, output: &Path) -> Result<std::process::Output> {
        let compiler = self.ensure_compiler()?;
        let mut cmd = Command::new(compiler);
        cmd.arg(source)
            .arg("-std=c++17")
            .arg("-O0")
            .arg("-Wall")
            .arg("-Wextra")
            .arg("-o")
            .arg(output)
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

    fn run_binary(&self, binary: &Path, args: &[String]) -> Result<std::process::Output> {
        let mut cmd = Command::new(binary);
        cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
        cmd.stdin(Stdio::inherit());
        cmd.output()
            .with_context(|| format!("failed to execute compiled binary {}", binary.display()))
    }

    fn binary_path(dir: &Path) -> PathBuf {
        let mut path = dir.join("run_cpp_binary");
        let suffix = std::env::consts::EXE_SUFFIX;
        if !suffix.is_empty() {
            if let Some(stripped) = suffix.strip_prefix('.') {
                path.set_extension(stripped);
            } else {
                path = PathBuf::from(format!("{}{}", path.display(), suffix));
            }
        }
        path
    }
}

impl LanguageEngine for CppEngine {
    fn id(&self) -> &'static str {
        "cpp"
    }

    fn display_name(&self) -> &'static str {
        "C++"
    }

    fn aliases(&self) -> &[&'static str] {
        &["c++"]
    }

    fn supports_sessions(&self) -> bool {
        self.compiler.is_some()
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
        let args = payload.args();

        // Try cache for inline/stdin payloads
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
            .prefix("run-cpp")
            .tempdir()
            .context("failed to create temporary directory for cpp build")?;
        let dir_path = temp_dir.path();

        let (source_path, cache_key) = match payload {
            ExecutionPayload::Inline { code, .. } | ExecutionPayload::Stdin { code, .. } => {
                let h = hash_source(code);
                (self.write_source(code, dir_path)?, Some(h))
            }
            ExecutionPayload::File { path, .. } => (self.copy_source(path, dir_path)?, None),
        };

        let binary_path = Self::binary_path(dir_path);
        let start = Instant::now();

        let compile_output = self.compile(&source_path, &binary_path)?;
        if !compile_output.status.success() {
            return Ok(ExecutionOutcome {
                language: self.id().to_string(),
                exit_code: compile_output.status.code(),
                stdout: String::from_utf8_lossy(&compile_output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&compile_output.stderr).into_owned(),
                duration: start.elapsed(),
            });
        }

        if let Some(h) = cache_key {
            cache_store(h, &binary_path);
        }

        let run_output = self.run_binary(&binary_path, args)?;
        Ok(ExecutionOutcome {
            language: self.id().to_string(),
            exit_code: run_output.status.code(),
            stdout: String::from_utf8_lossy(&run_output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&run_output.stderr).into_owned(),
            duration: start.elapsed(),
        })
    }

    fn start_session(&self) -> Result<Box<dyn LanguageSession>> {
        let compiler = self.ensure_compiler().map(Path::to_path_buf)?;

        let temp_dir = Builder::new()
            .prefix("run-cpp-repl")
            .tempdir()
            .context("failed to create temporary directory for cpp repl")?;
        let dir_path = temp_dir.path();
        let source_path = dir_path.join("main.cpp");
        let binary_path = Self::binary_path(dir_path);

        Ok(Box::new(CppSession {
            compiler,
            _temp_dir: temp_dir,
            source_path,
            binary_path,
            definitions: Vec::new(),
            statements: Vec::new(),
            previous_stdout: String::new(),
            previous_stderr: String::new(),
        }))
    }
}

fn resolve_cpp_compiler() -> Option<PathBuf> {
    ["c++", "clang++", "g++"]
        .into_iter()
        .find_map(|candidate| which::which(candidate).ok())
}

const SESSION_PREAMBLE: &str = concat!(
    "#include <iostream>\n",
    "#include <iomanip>\n",
    "#include <string>\n",
    "#include <vector>\n",
    "#include <map>\n",
    "#include <set>\n",
    "#include <unordered_map>\n",
    "#include <unordered_set>\n",
    "#include <deque>\n",
    "#include <list>\n",
    "#include <queue>\n",
    "#include <stack>\n",
    "#include <memory>\n",
    "#include <functional>\n",
    "#include <algorithm>\n",
    "#include <numeric>\n",
    "#include <cmath>\n\n",
    "using namespace std;\n\n",
);

struct CppSession {
    compiler: PathBuf,
    _temp_dir: TempDir,
    source_path: PathBuf,
    binary_path: PathBuf,
    definitions: Vec<String>,
    statements: Vec<String>,
    previous_stdout: String,
    previous_stderr: String,
}

impl CppSession {
    fn render_prelude(&self) -> String {
        let mut source = String::from(SESSION_PREAMBLE);
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
        source.push_str("int main()\n{\n    ios::sync_with_stdio(false);\n    cin.tie(nullptr);\n    cout.setf(std::ios::boolalpha);\n");
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
        source.push_str("    return 0;\n}\n");
        source
    }

    fn write_source(&self, contents: &str) -> Result<()> {
        fs::write(&self.source_path, contents).with_context(|| {
            format!(
                "failed to write generated C++ REPL source to {}",
                self.source_path.display()
            )
        })
    }

    fn compile_and_run(&mut self) -> Result<(std::process::Output, Duration)> {
        let start = Instant::now();
        let source = self.render_source();
        self.write_source(&source)?;
        let compile_output =
            invoke_cpp_compiler(&self.compiler, &self.source_path, &self.binary_path)?;
        if !compile_output.status.success() {
            let duration = start.elapsed();
            return Ok((compile_output, duration));
        }
        let execution_output = run_cpp_binary(&self.binary_path)?;
        let duration = start.elapsed();
        Ok((execution_output, duration))
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
            .join("standalone.cpp");
        fs::write(&standalone_path, &source)
            .with_context(|| "failed to write standalone C++ source".to_string())?;

        let compile_output =
            invoke_cpp_compiler(&self.compiler, &standalone_path, &self.binary_path)?;
        if !compile_output.status.success() {
            return Ok(ExecutionOutcome {
                language: "cpp".to_string(),
                exit_code: compile_output.status.code(),
                stdout: normalize_output(&compile_output.stdout),
                stderr: normalize_output(&compile_output.stderr),
                duration: start.elapsed(),
            });
        }

        let run_output = run_cpp_binary(&self.binary_path)?;
        Ok(ExecutionOutcome {
            language: "cpp".to_string(),
            exit_code: run_output.status.code(),
            stdout: normalize_output(&run_output.stdout),
            stderr: normalize_output(&run_output.stderr),
            duration: start.elapsed(),
        })
    }

    fn reset_state(&mut self) -> Result<()> {
        self.definitions.clear();
        self.statements.clear();
        self.previous_stdout.clear();
        self.previous_stderr.clear();
        let source = self.render_source();
        self.write_source(&source)
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
            language: "cpp".to_string(),
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
}

impl LanguageSession for CppSession {
    fn language_id(&self) -> &str {
        "cpp"
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
                    "C++ commands:\n  :reset - clear session state\n  :help  - show this message\n"
                        .to_string(),
                stderr: String::new(),
                duration: Duration::default(),
            });
        }

        if contains_main_definition(code) {
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
                let wrapped = wrap_cpp_expression(trimmed);
                self.add_statement(wrapped);
                let (output, duration) = self.compile_and_run()?;
                if !output.status.success() {
                    self.remove_last_statement();
                    return Ok(self.diff_outputs(&output, duration));
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SnippetKind {
    Definition,
    Statement,
    Expression,
}

fn classify_snippet(code: &str) -> SnippetKind {
    let trimmed = code.trim();
    if trimmed.starts_with("#include")
        || trimmed.starts_with("using ")
        || trimmed.starts_with("namespace ")
        || trimmed.starts_with("class ")
        || trimmed.starts_with("struct ")
        || trimmed.starts_with("enum ")
        || trimmed.starts_with("template ")
        || trimmed.ends_with("};")
    {
        return SnippetKind::Definition;
    }

    if trimmed.contains('{') && trimmed.contains('}') && trimmed.contains('(') {
        const CONTROL_KEYWORDS: [&str; 8] =
            ["if", "for", "while", "switch", "do", "else", "try", "catch"];
        let first = trimmed.split_whitespace().next().unwrap_or("");
        if !CONTROL_KEYWORDS.iter().any(|kw| {
            first == *kw
                || trimmed.starts_with(&format!("{} ", kw))
                || trimmed.starts_with(&format!("{}(", kw))
        }) {
            return SnippetKind::Definition;
        }
    }

    if is_cpp_expression(trimmed) {
        return SnippetKind::Expression;
    }

    SnippetKind::Statement
}

fn is_cpp_expression(code: &str) -> bool {
    if code.contains('\n') {
        return false;
    }
    if code.ends_with(';') {
        return false;
    }
    if code.starts_with("return ") {
        return false;
    }
    if code.starts_with("if ")
        || code.starts_with("for ")
        || code.starts_with("while ")
        || code.starts_with("switch ")
        || code.starts_with("do ")
        || code.starts_with("auto ")
    {
        return false;
    }
    if code.starts_with("std::") && code.contains('(') {
        return false;
    }
    if code.starts_with("cout") || code.starts_with("cin") {
        return false;
    }
    if code.starts_with('"') && code.ends_with('"') {
        return true;
    }
    if code.parse::<f64>().is_ok() {
        return true;
    }
    if code == "true" || code == "false" {
        return true;
    }
    if code.contains("==") || code.contains("!=") || code.contains("<=") || code.contains(">=") {
        return true;
    }
    if code.chars().any(|c| "+-*/%<>^|&".contains(c)) {
        return true;
    }
    if code
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
    {
        return true;
    }
    false
}

fn contains_main_definition(code: &str) -> bool {
    let bytes = code.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut in_string = false;
    let mut string_delim = b'"';
    let mut in_char = false;

    while i < len {
        let b = bytes[i];

        if in_line_comment {
            if b == b'\n' {
                in_line_comment = false;
            }
            i += 1;
            continue;
        }

        if in_block_comment {
            if b == b'*' && i + 1 < len && bytes[i + 1] == b'/' {
                in_block_comment = false;
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }

        if in_string {
            if b == b'\\' {
                i = (i + 2).min(len);
                continue;
            }
            if b == string_delim {
                in_string = false;
            }
            i += 1;
            continue;
        }

        if in_char {
            if b == b'\\' {
                i = (i + 2).min(len);
                continue;
            }
            if b == b'\'' {
                in_char = false;
            }
            i += 1;
            continue;
        }

        match b {
            b'/' if i + 1 < len && bytes[i + 1] == b'/' => {
                in_line_comment = true;
                i += 2;
                continue;
            }
            b'/' if i + 1 < len && bytes[i + 1] == b'*' => {
                in_block_comment = true;
                i += 2;
                continue;
            }
            b'"' | b'\'' => {
                if b == b'"' {
                    in_string = true;
                    string_delim = b;
                } else {
                    in_char = true;
                }
                i += 1;
                continue;
            }
            b'm' if i + 4 <= len && &bytes[i..i + 4] == b"main" => {
                if i > 0 {
                    let prev = bytes[i - 1];
                    if prev.is_ascii_alphanumeric() || prev == b'_' {
                        i += 1;
                        continue;
                    }
                }

                let after_name = i + 4;
                if after_name < len {
                    let next = bytes[after_name];
                    if next.is_ascii_alphanumeric() || next == b'_' {
                        i += 1;
                        continue;
                    }
                }

                let mut j = after_name;
                while j < len && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j >= len || bytes[j] != b'(' {
                    i += 1;
                    continue;
                }

                let mut depth = 1usize;
                let mut k = j + 1;
                let mut inner_line_comment = false;
                let mut inner_block_comment = false;
                let mut inner_string = false;
                let mut inner_char = false;

                while k < len {
                    let ch = bytes[k];

                    if inner_line_comment {
                        if ch == b'\n' {
                            inner_line_comment = false;
                        }
                        k += 1;
                        continue;
                    }

                    if inner_block_comment {
                        if ch == b'*' && k + 1 < len && bytes[k + 1] == b'/' {
                            inner_block_comment = false;
                            k += 2;
                            continue;
                        }
                        k += 1;
                        continue;
                    }

                    if inner_string {
                        if ch == b'\\' {
                            k = (k + 2).min(len);
                            continue;
                        }
                        if ch == b'"' {
                            inner_string = false;
                        }
                        k += 1;
                        continue;
                    }

                    if inner_char {
                        if ch == b'\\' {
                            k = (k + 2).min(len);
                            continue;
                        }
                        if ch == b'\'' {
                            inner_char = false;
                        }
                        k += 1;
                        continue;
                    }

                    match ch {
                        b'/' if k + 1 < len && bytes[k + 1] == b'/' => {
                            inner_line_comment = true;
                            k += 2;
                            continue;
                        }
                        b'/' if k + 1 < len && bytes[k + 1] == b'*' => {
                            inner_block_comment = true;
                            k += 2;
                            continue;
                        }
                        b'"' => {
                            inner_string = true;
                            k += 1;
                            continue;
                        }
                        b'\'' => {
                            inner_char = true;
                            k += 1;
                            continue;
                        }
                        b'(' => {
                            depth += 1;
                        }
                        b')' => {
                            depth -= 1;
                            k += 1;
                            if depth == 0 {
                                break;
                            } else {
                                continue;
                            }
                        }
                        _ => {}
                    }

                    k += 1;
                }

                if depth != 0 {
                    i += 1;
                    continue;
                }

                let mut after = k;
                loop {
                    while after < len && bytes[after].is_ascii_whitespace() {
                        after += 1;
                    }
                    if after + 1 < len && bytes[after] == b'/' && bytes[after + 1] == b'/' {
                        after += 2;
                        while after < len && bytes[after] != b'\n' {
                            after += 1;
                        }
                        continue;
                    }
                    if after + 1 < len && bytes[after] == b'/' && bytes[after + 1] == b'*' {
                        after += 2;
                        while after + 1 < len {
                            if bytes[after] == b'*' && bytes[after + 1] == b'/' {
                                after += 2;
                                break;
                            }
                            after += 1;
                        }
                        continue;
                    }
                    break;
                }

                while after < len {
                    match bytes[after] {
                        b'{' => return true,
                        b';' => break,
                        b'/' if after + 1 < len && bytes[after + 1] == b'/' => {
                            after += 2;
                            while after < len && bytes[after] != b'\n' {
                                after += 1;
                            }
                        }
                        b'/' if after + 1 < len && bytes[after + 1] == b'*' => {
                            after += 2;
                            while after + 1 < len {
                                if bytes[after] == b'*' && bytes[after + 1] == b'/' {
                                    after += 2;
                                    break;
                                }
                                after += 1;
                            }
                        }
                        b'"' => {
                            after += 1;
                            while after < len {
                                if bytes[after] == b'"' {
                                    after += 1;
                                    break;
                                }
                                if bytes[after] == b'\\' {
                                    after = (after + 2).min(len);
                                } else {
                                    after += 1;
                                }
                            }
                        }
                        b'\'' => {
                            after += 1;
                            while after < len {
                                if bytes[after] == b'\'' {
                                    after += 1;
                                    break;
                                }
                                if bytes[after] == b'\\' {
                                    after = (after + 2).min(len);
                                } else {
                                    after += 1;
                                }
                            }
                        }
                        b'-' if after + 1 < len && bytes[after + 1] == b'>' => {
                            after += 2;
                        }
                        b'(' => {
                            let mut depth = 1usize;
                            after += 1;
                            while after < len && depth > 0 {
                                match bytes[after] {
                                    b'(' => depth += 1,
                                    b')' => depth -= 1,
                                    b'"' => {
                                        after += 1;
                                        while after < len {
                                            if bytes[after] == b'"' {
                                                after += 1;
                                                break;
                                            }
                                            if bytes[after] == b'\\' {
                                                after = (after + 2).min(len);
                                            } else {
                                                after += 1;
                                            }
                                        }
                                        continue;
                                    }
                                    b'\'' => {
                                        after += 1;
                                        while after < len {
                                            if bytes[after] == b'\'' {
                                                after += 1;
                                                break;
                                            }
                                            if bytes[after] == b'\\' {
                                                after = (after + 2).min(len);
                                            } else {
                                                after += 1;
                                            }
                                        }
                                        continue;
                                    }
                                    _ => {}
                                }
                                after += 1;
                            }
                        }
                        _ => {
                            after += 1;
                        }
                    }
                }
            }
            _ => {}
        }

        i += 1;
    }

    false
}

fn wrap_cpp_expression(code: &str) -> String {
    format!("std::cout << ({code}) << std::endl;\n")
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

fn invoke_cpp_compiler(
    compiler: &Path,
    source: &Path,
    output: &Path,
) -> Result<std::process::Output> {
    let mut cmd = Command::new(compiler);
    cmd.arg(source)
        .arg("-std=c++17")
        .arg("-O0")
        .arg("-Wall")
        .arg("-Wextra")
        .arg("-o")
        .arg(output)
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

fn run_cpp_binary(binary: &Path) -> Result<std::process::Output> {
    let mut cmd = Command::new(binary);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd.output()
        .with_context(|| format!("failed to execute compiled binary {}", binary.display()))
}
