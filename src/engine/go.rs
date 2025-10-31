use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use anyhow::{Context, Result};
use tempfile::{Builder, TempDir};

use super::{ExecutionOutcome, ExecutionPayload, LanguageEngine, LanguageSession};

pub struct GoEngine {
    executable: Option<PathBuf>,
}

impl GoEngine {
    pub fn new() -> Self {
        Self {
            executable: resolve_go_binary(),
        }
    }

    fn ensure_executable(&self) -> Result<&Path> {
        self.executable.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "Go support requires the `go` executable. Install it from https://go.dev/dl/ and ensure it is on your PATH."
            )
        })
    }

    fn write_temp_source(&self, code: &str) -> Result<(tempfile::TempDir, PathBuf)> {
        let dir = Builder::new()
            .prefix("run-go")
            .tempdir()
            .context("failed to create temporary directory for go source")?;
        let path = dir.path().join("main.go");
        let mut contents = code.to_string();
        if !contents.ends_with('\n') {
            contents.push('\n');
        }
        std::fs::write(&path, contents).with_context(|| {
            format!("failed to write temporary Go source to {}", path.display())
        })?;
        Ok((dir, path))
    }

    fn execute_with_path(&self, binary: &Path, source: &Path) -> Result<std::process::Output> {
        let mut cmd = Command::new(binary);
        cmd.arg("run")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("GO111MODULE", "off");
        cmd.stdin(Stdio::inherit());

        if let Some(parent) = source.parent() {
            cmd.current_dir(parent);
            if let Some(file_name) = source.file_name() {
                cmd.arg(file_name);
            } else {
                cmd.arg(source);
            }
        } else {
            cmd.arg(source);
        }
        cmd.output().with_context(|| {
            format!(
                "failed to invoke {} to run {}",
                binary.display(),
                source.display()
            )
        })
    }
}

impl LanguageEngine for GoEngine {
    fn id(&self) -> &'static str {
        "go"
    }

    fn display_name(&self) -> &'static str {
        "Go"
    }

    fn aliases(&self) -> &[&'static str] {
        &["golang"]
    }

    fn supports_sessions(&self) -> bool {
        true
    }

    fn validate(&self) -> Result<()> {
        let binary = self.ensure_executable()?;
        let mut cmd = Command::new(binary);
        cmd.arg("version")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        cmd.status()
            .with_context(|| format!("failed to invoke {}", binary.display()))?
            .success()
            .then_some(())
            .ok_or_else(|| anyhow::anyhow!("{} is not executable", binary.display()))
    }

    fn execute(&self, payload: &ExecutionPayload) -> Result<ExecutionOutcome> {
        let binary = self.ensure_executable()?;
        let start = Instant::now();

        let (temp_dir, source_path) = match payload {
            ExecutionPayload::Inline { code } => {
                let (dir, path) = self.write_temp_source(code)?;
                (Some(dir), path)
            }
            ExecutionPayload::Stdin { code } => {
                let (dir, path) = self.write_temp_source(code)?;
                (Some(dir), path)
            }
            ExecutionPayload::File { path } => (None, path.clone()),
        };

        let output = self.execute_with_path(binary, &source_path)?;

        // Ensure temp_dir stays in scope until after the command runs
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
        let binary = self.ensure_executable()?.to_path_buf();
        let session = GoSession::new(binary)?;
        Ok(Box::new(session))
    }
}

fn resolve_go_binary() -> Option<PathBuf> {
    which::which("go").ok()
}

fn import_is_used_in_code(import: &str, code: &str) -> bool {
    let import_trimmed = import.trim().trim_matches('"');
    let package_name = import_trimmed.rsplit('/').next().unwrap_or(import_trimmed);
    let pattern = format!("{}.", package_name);
    code.contains(&pattern)
}

const SESSION_MAIN_FILE: &str = "main.go";

struct GoSession {
    go_binary: PathBuf,
    workspace: TempDir,
    imports: BTreeSet<String>,
    items: Vec<String>,
    statements: Vec<String>,
    last_stdout: String,
    last_stderr: String,
}

enum GoSnippetKind {
    Import(Option<String>),
    Item,
    Statement,
}

impl GoSession {
    fn new(go_binary: PathBuf) -> Result<Self> {
        let workspace = TempDir::new().context("failed to create Go session workspace")?;
        let mut imports = BTreeSet::new();
        imports.insert("\"fmt\"".to_string());
        let session = Self {
            go_binary,
            workspace,
            imports,
            items: Vec::new(),
            statements: Vec::new(),
            last_stdout: String::new(),
            last_stderr: String::new(),
        };
        session.persist_source()?;
        Ok(session)
    }

    fn language_id(&self) -> &str {
        "go"
    }

    fn source_path(&self) -> PathBuf {
        self.workspace.path().join(SESSION_MAIN_FILE)
    }

    fn persist_source(&self) -> Result<()> {
        let source = self.render_source();
        fs::write(self.source_path(), source)
            .with_context(|| "failed to write Go session source".to_string())
    }

    fn render_source(&self) -> String {
        let mut source = String::from("package main\n\n");

        if !self.imports.is_empty() {
            source.push_str("import (\n");
            for import in &self.imports {
                source.push_str("\t");
                source.push_str(import);
                source.push('\n');
            }
            source.push_str(")\n\n");
        }

        source.push_str(concat!(
            "func __print(value interface{}) {\n",
            "\tif s, ok := value.(string); ok {\n",
            "\t\tfmt.Println(s)\n",
            "\t\treturn\n",
            "\t}\n",
            "\tfmt.Printf(\"%#v\\n\", value)\n",
            "}\n\n",
        ));

        for item in &self.items {
            source.push_str(item);
            if !item.ends_with('\n') {
                source.push('\n');
            }
            source.push('\n');
        }

        source.push_str("func main() {\n");
        if self.statements.is_empty() {
            source.push_str("\t// session body\n");
        } else {
            for snippet in &self.statements {
                for line in snippet.lines() {
                    source.push('\t');
                    source.push_str(line);
                    source.push('\n');
                }
            }
        }
        source.push_str("}\n");

        source
    }

    fn run_program(&self) -> Result<std::process::Output> {
        let mut cmd = Command::new(&self.go_binary);
        cmd.arg("run")
            .arg(SESSION_MAIN_FILE)
            .env("GO111MODULE", "off")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(self.workspace.path());
        cmd.output().with_context(|| {
            format!(
                "failed to execute {} for Go session",
                self.go_binary.display()
            )
        })
    }

    fn run_standalone_program(&self, code: &str) -> Result<ExecutionOutcome> {
        let start = Instant::now();
        let standalone_path = self.workspace.path().join("standalone.go");

        let source = if has_package_declaration(code) {
            let mut snippet = code.to_string();
            if !snippet.ends_with('\n') {
                snippet.push('\n');
            }
            snippet
        } else {
            let mut source = String::from("package main\n\n");

            let used_imports: Vec<_> = self
                .imports
                .iter()
                .filter(|import| import_is_used_in_code(import, code))
                .cloned()
                .collect();

            if !used_imports.is_empty() {
                source.push_str("import (\n");
                for import in &used_imports {
                    source.push_str("\t");
                    source.push_str(import);
                    source.push('\n');
                }
                source.push_str(")\n\n");
            }

            source.push_str(code);
            if !code.ends_with('\n') {
                source.push('\n');
            }
            source
        };

        fs::write(&standalone_path, source)
            .with_context(|| "failed to write Go standalone source".to_string())?;

        let mut cmd = Command::new(&self.go_binary);
        cmd.arg("run")
            .arg("standalone.go")
            .env("GO111MODULE", "off")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(self.workspace.path());

        let output = cmd.output().with_context(|| {
            format!(
                "failed to execute {} for Go standalone program",
                self.go_binary.display()
            )
        })?;

        let outcome = ExecutionOutcome {
            language: self.language_id().to_string(),
            exit_code: output.status.code(),
            stdout: Self::normalize_output(&output.stdout),
            stderr: Self::normalize_output(&output.stderr),
            duration: start.elapsed(),
        };

        let _ = fs::remove_file(&standalone_path);

        Ok(outcome)
    }

    fn add_import(&mut self, spec: &str) -> GoSnippetKind {
        let added = self.imports.insert(spec.to_string());
        if added {
            GoSnippetKind::Import(Some(spec.to_string()))
        } else {
            GoSnippetKind::Import(None)
        }
    }

    fn add_item(&mut self, code: &str) -> GoSnippetKind {
        let mut snippet = code.to_string();
        if !snippet.ends_with('\n') {
            snippet.push('\n');
        }
        self.items.push(snippet);
        GoSnippetKind::Item
    }

    fn add_statement(&mut self, code: &str) -> GoSnippetKind {
        let snippet = sanitize_statement(code);
        self.statements.push(snippet);
        GoSnippetKind::Statement
    }

    fn add_expression(&mut self, code: &str) -> GoSnippetKind {
        let wrapped = wrap_expression(code);
        self.statements.push(wrapped);
        GoSnippetKind::Statement
    }

    fn rollback(&mut self, kind: GoSnippetKind) -> Result<()> {
        match kind {
            GoSnippetKind::Import(Some(spec)) => {
                self.imports.remove(&spec);
            }
            GoSnippetKind::Import(None) => {}
            GoSnippetKind::Item => {
                self.items.pop();
            }
            GoSnippetKind::Statement => {
                self.statements.pop();
            }
        }
        self.persist_source()
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

    fn run_insertion(&mut self, kind: GoSnippetKind) -> Result<(ExecutionOutcome, bool)> {
        match kind {
            GoSnippetKind::Import(None) => Ok((
                ExecutionOutcome {
                    language: self.language_id().to_string(),
                    exit_code: None,
                    stdout: String::new(),
                    stderr: String::new(),
                    duration: Default::default(),
                },
                true,
            )),
            other_kind => {
                self.persist_source()?;
                let start = Instant::now();
                let output = self.run_program()?;

                let stdout_full = Self::normalize_output(&output.stdout);
                let stderr_full = Self::normalize_output(&output.stderr);

                let stdout = Self::diff_outputs(&self.last_stdout, &stdout_full);
                let stderr = Self::diff_outputs(&self.last_stderr, &stderr_full);
                let duration = start.elapsed();

                if output.status.success() {
                    self.last_stdout = stdout_full;
                    self.last_stderr = stderr_full;
                    let outcome = ExecutionOutcome {
                        language: self.language_id().to_string(),
                        exit_code: output.status.code(),
                        stdout,
                        stderr,
                        duration,
                    };
                    return Ok((outcome, true));
                }

                if matches!(&other_kind, GoSnippetKind::Import(Some(_)))
                    && stderr_full.contains("imported and not used")
                {
                    return Ok((
                        ExecutionOutcome {
                            language: self.language_id().to_string(),
                            exit_code: None,
                            stdout: String::new(),
                            stderr: String::new(),
                            duration,
                        },
                        true,
                    ));
                }

                self.rollback(other_kind)?;
                let outcome = ExecutionOutcome {
                    language: self.language_id().to_string(),
                    exit_code: output.status.code(),
                    stdout,
                    stderr,
                    duration,
                };
                Ok((outcome, false))
            }
        }
    }

    fn run_import(&mut self, spec: &str) -> Result<(ExecutionOutcome, bool)> {
        let kind = self.add_import(spec);
        self.run_insertion(kind)
    }

    fn run_item(&mut self, code: &str) -> Result<(ExecutionOutcome, bool)> {
        let kind = self.add_item(code);
        self.run_insertion(kind)
    }

    fn run_statement(&mut self, code: &str) -> Result<(ExecutionOutcome, bool)> {
        let kind = self.add_statement(code);
        self.run_insertion(kind)
    }

    fn run_expression(&mut self, code: &str) -> Result<(ExecutionOutcome, bool)> {
        let kind = self.add_expression(code);
        self.run_insertion(kind)
    }
}

impl LanguageSession for GoSession {
    fn language_id(&self) -> &str {
        GoSession::language_id(self)
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

        if trimmed.starts_with("package ") && !trimmed.contains('\n') {
            return Ok(ExecutionOutcome {
                language: self.language_id().to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                duration: Instant::now().elapsed(),
            });
        }

        if contains_main_definition(trimmed) {
            let outcome = self.run_standalone_program(code)?;
            return Ok(outcome);
        }

        if let Some(import) = parse_import_spec(trimmed) {
            let (outcome, _) = self.run_import(&import)?;
            return Ok(outcome);
        }

        if is_item_snippet(trimmed) {
            let (outcome, _) = self.run_item(code)?;
            return Ok(outcome);
        }

        if should_treat_as_expression(trimmed) {
            let (outcome, success) = self.run_expression(trimmed)?;
            if success {
                return Ok(outcome);
            }
        }

        let (outcome, _) = self.run_statement(code)?;
        Ok(outcome)
    }

    fn shutdown(&mut self) -> Result<()> {
        Ok(())
    }
}

fn parse_import_spec(code: &str) -> Option<String> {
    let trimmed = code.trim_start();
    if !trimmed.starts_with("import ") {
        return None;
    }
    let rest = trimmed.trim_start_matches("import").trim();
    if rest.is_empty() || rest.starts_with('(') {
        return None;
    }
    Some(rest.to_string())
}

fn is_item_snippet(code: &str) -> bool {
    let trimmed = code.trim_start();
    if trimmed.is_empty() {
        return false;
    }
    const KEYWORDS: [&str; 6] = ["type", "const", "var", "func", "package", "import"];
    KEYWORDS.iter().any(|kw| {
        trimmed.starts_with(kw)
            && trimmed
                .chars()
                .nth(kw.len())
                .map(|ch| ch.is_whitespace() || ch == '(')
                .unwrap_or(true)
    })
}

fn should_treat_as_expression(code: &str) -> bool {
    let trimmed = code.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.contains('\n') {
        return false;
    }
    if trimmed.ends_with(';') {
        return false;
    }
    if trimmed.contains(":=") {
        return false;
    }
    if trimmed.contains('=') && !trimmed.contains("==") {
        return false;
    }
    const RESERVED: [&str; 8] = [
        "if ", "for ", "switch ", "select ", "return ", "go ", "defer ", "var ",
    ];
    if RESERVED.iter().any(|kw| trimmed.starts_with(kw)) {
        return false;
    }
    true
}

fn wrap_expression(code: &str) -> String {
    format!("__print({});\n", code)
}

fn sanitize_statement(code: &str) -> String {
    let mut snippet = code.to_string();
    if !snippet.ends_with('\n') {
        snippet.push('\n');
    }

    let trimmed = code.trim();
    if trimmed.is_empty() || trimmed.contains('\n') {
        return snippet;
    }

    let mut identifiers: Vec<String> = Vec::new();

    if let Some(idx) = trimmed.find(" :=") {
        let lhs = &trimmed[..idx];
        identifiers = lhs
            .split(',')
            .map(|part| part.trim())
            .filter(|name| !name.is_empty() && *name != "_")
            .map(|name| name.to_string())
            .collect();
    } else if let Some(idx) = trimmed.find(':') {
        if trimmed[idx..].starts_with(":=") {
            let lhs = &trimmed[..idx];
            identifiers = lhs
                .split(',')
                .map(|part| part.trim())
                .filter(|name| !name.is_empty() && *name != "_")
                .map(|name| name.to_string())
                .collect();
        }
    } else if trimmed.starts_with("var ") {
        let rest = trimmed[4..].trim();
        if !rest.starts_with('(') {
            let names_part = rest.split('=').next().unwrap_or(rest).trim();
            identifiers = names_part
                .split(',')
                .filter_map(|segment| {
                    let token = segment.trim().split_whitespace().next().unwrap_or("");
                    if token.is_empty() || token == "_" {
                        None
                    } else {
                        Some(token.to_string())
                    }
                })
                .collect();
        }
    } else if trimmed.starts_with("const ") {
        let rest = trimmed[6..].trim();
        if !rest.starts_with('(') {
            let names_part = rest.split('=').next().unwrap_or(rest).trim();
            identifiers = names_part
                .split(',')
                .filter_map(|segment| {
                    let token = segment.trim().split_whitespace().next().unwrap_or("");
                    if token.is_empty() || token == "_" {
                        None
                    } else {
                        Some(token.to_string())
                    }
                })
                .collect();
        }
    }

    if identifiers.is_empty() {
        return snippet;
    }

    for name in identifiers {
        snippet.push_str("_ = ");
        snippet.push_str(&name);
        snippet.push('\n');
    }

    snippet
}

fn has_package_declaration(code: &str) -> bool {
    code.lines()
        .any(|line| line.trim_start().starts_with("package "))
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
            b'"' | b'`' => {
                in_string = true;
                string_delim = b;
                i += 1;
                continue;
            }
            b'\'' => {
                in_char = true;
                i += 1;
                continue;
            }
            b'f' if i + 4 <= len && &bytes[i..i + 4] == b"func" => {
                if i > 0 {
                    let prev = bytes[i - 1];
                    if prev.is_ascii_alphanumeric() || prev == b'_' {
                        i += 1;
                        continue;
                    }
                }

                let mut j = i + 4;
                while j < len && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }

                if j + 4 > len || &bytes[j..j + 4] != b"main" {
                    i += 1;
                    continue;
                }

                let after = j + 4;
                if after < len {
                    let ch = bytes[after];
                    if ch.is_ascii_alphanumeric() || ch == b'_' {
                        i += 1;
                        continue;
                    }
                }

                let mut k = after;
                while k < len && bytes[k].is_ascii_whitespace() {
                    k += 1;
                }
                if k < len && bytes[k] == b'(' {
                    return true;
                }
            }
            _ => {}
        }

        i += 1;
    }

    false
}
