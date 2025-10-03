use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use tempfile::{Builder, TempDir};

use super::{ExecutionOutcome, ExecutionPayload, LanguageEngine, LanguageSession};

pub struct CSharpEngine {
    runtime: Option<PathBuf>,
    target_framework: Option<String>,
}

impl CSharpEngine {
    pub fn new() -> Self {
        let runtime = resolve_dotnet_runtime();
        let target_framework = runtime
            .as_ref()
            .and_then(|path| detect_target_framework(path).ok());
        Self {
            runtime,
            target_framework,
        }
    }

    fn ensure_runtime(&self) -> Result<&Path> {
        self.runtime.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "C# support requires the `dotnet` CLI. Install the .NET SDK from https://dotnet.microsoft.com/download and ensure `dotnet` is on your PATH."
            )
        })
    }

    fn ensure_target_framework(&self) -> Result<&str> {
        self.target_framework
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Unable to detect installed .NET SDK target framework"))
    }

    fn prepare_source(&self, payload: &ExecutionPayload, dir: &Path) -> Result<PathBuf> {
        let target = dir.join("Program.cs");
        match payload {
            ExecutionPayload::Inline { code } | ExecutionPayload::Stdin { code } => {
                let mut contents = code.to_string();
                if !contents.ends_with('\n') {
                    contents.push('\n');
                }
                fs::write(&target, contents).with_context(|| {
                    format!(
                        "failed to write temporary C# source to {}",
                        target.display()
                    )
                })?;
            }
            ExecutionPayload::File { path } => {
                fs::copy(path, &target).with_context(|| {
                    format!(
                        "failed to copy C# source from {} to {}",
                        path.display(),
                        target.display()
                    )
                })?;
            }
        }
        Ok(target)
    }

    fn write_project_file(&self, dir: &Path, tfm: &str) -> Result<PathBuf> {
        let project_path = dir.join("Run.csproj");
        let contents = format!(
            r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>{}</TargetFramework>
    <ImplicitUsings>enable</ImplicitUsings>
    <Nullable>disable</Nullable>
        <NoWarn>CS0219;CS8321</NoWarn>
  </PropertyGroup>
</Project>
"#,
            tfm
        );
        fs::write(&project_path, contents).with_context(|| {
            format!(
                "failed to write temporary C# project file to {}",
                project_path.display()
            )
        })?;
        Ok(project_path)
    }

    fn run_project(
        &self,
        runtime: &Path,
        project: &Path,
        workdir: &Path,
    ) -> Result<std::process::Output> {
        let mut cmd = Command::new(runtime);
        cmd.arg("run")
            .arg("--project")
            .arg(project)
            .arg("--nologo")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(workdir);
        cmd.stdin(Stdio::inherit());
        cmd.env("DOTNET_CLI_TELEMETRY_OPTOUT", "1");
        cmd.env("DOTNET_SKIP_FIRST_TIME_EXPERIENCE", "1");
        cmd.output().with_context(|| {
            format!(
                "failed to execute dotnet run for project {} using {}",
                project.display(),
                runtime.display()
            )
        })
    }
}

impl LanguageEngine for CSharpEngine {
    fn id(&self) -> &'static str {
        "csharp"
    }

    fn display_name(&self) -> &'static str {
        "C#"
    }

    fn aliases(&self) -> &[&'static str] {
        &["cs", "c#", "dotnet"]
    }

    fn supports_sessions(&self) -> bool {
        self.runtime.is_some() && self.target_framework.is_some()
    }

    fn validate(&self) -> Result<()> {
        let runtime = self.ensure_runtime()?;
        let _tfm = self.ensure_target_framework()?;

        let mut cmd = Command::new(runtime);
        cmd.arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        cmd.status()
            .with_context(|| format!("failed to invoke {}", runtime.display()))?
            .success()
            .then_some(())
            .ok_or_else(|| anyhow::anyhow!("{} is not executable", runtime.display()))
    }

    fn execute(&self, payload: &ExecutionPayload) -> Result<ExecutionOutcome> {
        let runtime = self.ensure_runtime()?;
        let tfm = self.ensure_target_framework()?;

        let build_dir = Builder::new()
            .prefix("run-csharp")
            .tempdir()
            .context("failed to create temporary directory for csharp build")?;
        let dir_path = build_dir.path();

        self.write_project_file(dir_path, tfm)?;
        self.prepare_source(payload, dir_path)?;

        let project_path = dir_path.join("Run.csproj");
        let start = Instant::now();

        let output = self.run_project(runtime, &project_path, dir_path)?;

        Ok(ExecutionOutcome {
            language: self.id().to_string(),
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            duration: start.elapsed(),
        })
    }

    fn start_session(&self) -> Result<Box<dyn LanguageSession>> {
        let runtime = self.ensure_runtime()?.to_path_buf();
        let tfm = self.ensure_target_framework()?.to_string();

        let dir = Builder::new()
            .prefix("run-csharp-repl")
            .tempdir()
            .context("failed to create temporary directory for csharp repl")?;
        let dir_path = dir.path();

        let project_path = self.write_project_file(dir_path, &tfm)?;
        let program_path = dir_path.join("Program.cs");
        fs::write(&program_path, "// C# REPL session\n")
            .with_context(|| format!("failed to initialize {}", program_path.display()))?;

        Ok(Box::new(CSharpSession {
            runtime,
            dir,
            project_path,
            program_path,
            snippets: Vec::new(),
            previous_stdout: String::new(),
            previous_stderr: String::new(),
        }))
    }
}

struct CSharpSession {
    runtime: PathBuf,
    dir: TempDir,
    project_path: PathBuf,
    program_path: PathBuf,
    snippets: Vec<String>,
    previous_stdout: String,
    previous_stderr: String,
}

impl CSharpSession {
    fn render_source(&self) -> String {
        let mut source = String::from(
            "using System;\nusing System.Collections.Generic;\nusing System.Linq;\nusing System.Threading.Tasks;\n#nullable disable\n",
        );
        for snippet in &self.snippets {
            source.push_str(snippet);
            if !snippet.ends_with('\n') {
                source.push('\n');
            }
        }
        source
    }

    fn write_source(&self, contents: &str) -> Result<()> {
        fs::write(&self.program_path, contents).with_context(|| {
            format!(
                "failed to write generated C# REPL source to {}",
                self.program_path.display()
            )
        })
    }

    fn run_current(&mut self, start: Instant) -> Result<(ExecutionOutcome, bool)> {
        let source = self.render_source();
        self.write_source(&source)?;

        let output = run_dotnet_project(&self.runtime, &self.project_path, self.dir.path())?;
        let stdout_full = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr_full = String::from_utf8_lossy(&output.stderr).into_owned();

        let stdout_delta = diff_output(&self.previous_stdout, &stdout_full);
        let stderr_delta = diff_output(&self.previous_stderr, &stderr_full);

        let success = output.status.success();
        if success {
            self.previous_stdout = stdout_full;
            self.previous_stderr = stderr_full;
        }

        let outcome = ExecutionOutcome {
            language: "csharp".to_string(),
            exit_code: output.status.code(),
            stdout: stdout_delta,
            stderr: stderr_delta,
            duration: start.elapsed(),
        };

        Ok((outcome, success))
    }

    fn run_snippet(&mut self, snippet: String) -> Result<ExecutionOutcome> {
        self.snippets.push(snippet);
        let start = Instant::now();
        let (outcome, success) = self.run_current(start)?;
        if !success {
            let _ = self.snippets.pop();
        }
        Ok(outcome)
    }

    fn reset_state(&mut self) -> Result<()> {
        self.snippets.clear();
        self.previous_stdout.clear();
        self.previous_stderr.clear();
        let source = self.render_source();
        self.write_source(&source)
    }
}

impl LanguageSession for CSharpSession {
    fn language_id(&self) -> &str {
        "csharp"
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
                    "C# commands:\n  :reset — clear session state\n  :help  — show this message\n"
                        .to_string(),
                stderr: String::new(),
                duration: Duration::default(),
            });
        }

        if should_treat_as_expression(trimmed) {
            let snippet = wrap_expression(trimmed, self.snippets.len());
            let outcome = self.run_snippet(snippet)?;
            if outcome.exit_code.unwrap_or(0) == 0 {
                return Ok(outcome);
            }
        }

        let snippet = prepare_statement(code);
        let outcome = self.run_snippet(snippet)?;
        Ok(outcome)
    }

    fn shutdown(&mut self) -> Result<()> {
        // TempDir cleanup handled automatically.
        Ok(())
    }
}

fn diff_output(previous: &str, current: &str) -> String {
    if let Some(stripped) = current.strip_prefix(previous) {
        stripped.to_string()
    } else {
        current.to_string()
    }
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
    let lowered = trimmed.to_ascii_lowercase();
    const KEYWORDS: [&str; 17] = [
        "using ",
        "namespace ",
        "class ",
        "struct ",
        "record ",
        "enum ",
        "interface ",
        "public ",
        "private ",
        "protected ",
        "internal ",
        "static ",
        "if ",
        "for ",
        "while ",
        "switch ",
        "try ",
    ];
    if KEYWORDS.iter().any(|kw| lowered.starts_with(kw)) {
        return false;
    }
    if lowered.starts_with("return ") || lowered.starts_with("throw ") {
        return false;
    }
    if trimmed.starts_with("Console.") || trimmed.starts_with("System.Console.") {
        return false;
    }

    if trimmed == "true" || trimmed == "false" {
        return true;
    }
    if trimmed.parse::<f64>().is_ok() {
        return true;
    }
    if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
        return true;
    }

    if trimmed.contains("==")
        || trimmed.contains("!=")
        || trimmed.contains("<=")
        || trimmed.contains(">=")
        || trimmed.contains("&&")
        || trimmed.contains("||")
    {
        return true;
    }
    if trimmed.chars().any(|c| "+-*/%<>^|&".contains(c)) {
        return true;
    }

    if trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
    {
        return true;
    }

    false
}

fn wrap_expression(code: &str, index: usize) -> String {
    format!("var __repl_val_{index} = ({code});\nConsole.WriteLine(__repl_val_{index});\n")
}

fn prepare_statement(code: &str) -> String {
    let mut snippet = code.to_string();
    if !snippet.ends_with('\n') {
        snippet.push('\n');
    }
    snippet
}

fn resolve_dotnet_runtime() -> Option<PathBuf> {
    which::which("dotnet").ok()
}

fn detect_target_framework(dotnet: &Path) -> Result<String> {
    let output = Command::new(dotnet)
        .arg("--list-sdks")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .with_context(|| format!("failed to query SDKs via {}", dotnet.display()))?;

    if !output.status.success() {
        bail!(
            "{} --list-sdks exited with status {}",
            dotnet.display(),
            output.status
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut best: Option<(u32, u32, String)> = None;

    for line in stdout.lines() {
        let version = line.split_whitespace().next().unwrap_or("");
        if version.is_empty() {
            continue;
        }
        if let Some((major, minor)) = parse_version(version) {
            let tfm = format!("net{}.{}", major, minor);
            match &best {
                Some((b_major, b_minor, _)) if (*b_major, *b_minor) >= (major, minor) => {}
                _ => best = Some((major, minor, tfm)),
            }
        }
    }

    best.map(|(_, _, tfm)| tfm).ok_or_else(|| {
        anyhow::anyhow!("unable to infer target framework from dotnet --list-sdks output")
    })
}

fn parse_version(version: &str) -> Option<(u32, u32)> {
    let mut parts = version.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor))
}

fn run_dotnet_project(
    runtime: &Path,
    project: &Path,
    workdir: &Path,
) -> Result<std::process::Output> {
    let mut cmd = Command::new(runtime);
    cmd.arg("run")
        .arg("--project")
        .arg(project)
        .arg("--nologo")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(workdir);
    cmd.env("DOTNET_CLI_TELEMETRY_OPTOUT", "1");
    cmd.env("DOTNET_SKIP_FIRST_TIME_EXPERIENCE", "1");
    cmd.output().with_context(|| {
        format!(
            "failed to execute dotnet run for project {} using {}",
            project.display(),
            runtime.display()
        )
    })
}
