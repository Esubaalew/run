use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use anyhow::{Result, ensure};
use clap::{Parser, ValueHint, builder::NonEmptyStringValueParser};

use crate::language::LanguageSpec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputSource {
    Inline(String),
    File(PathBuf),
    Stdin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionSpec {
    pub language: Option<LanguageSpec>,
    pub source: InputSource,
    pub detect_language: bool,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Execute(ExecutionSpec),
    Repl {
        initial_language: Option<LanguageSpec>,
        detect_language: bool,
    },
    ShowVersion,
    CheckToolchains,
    ShowVersions {
        language: Option<LanguageSpec>,
    },
    Install {
        language: Option<LanguageSpec>,
        package: String,
    },
    Bench {
        spec: ExecutionSpec,
        iterations: u32,
    },
    Watch {
        spec: ExecutionSpec,
    },
}

pub fn parse() -> Result<Command> {
    let cli = Cli::parse();

    if cli.version {
        return Ok(Command::ShowVersion);
    }
    if cli.check {
        return Ok(Command::CheckToolchains);
    }
    if cli.versions {
        ensure!(
            cli.code.is_none() && cli.file.is_none(),
            "--versions does not accept --code or --file"
        );
        let mut language = cli
            .lang
            .as_ref()
            .map(|value| LanguageSpec::new(value.to_string()));
        let mut trailing = cli.args.clone();
        if language.is_none()
            && trailing.len() == 1
            && crate::language::is_language_token(&trailing[0])
        {
            let raw = trailing.remove(0);
            language = Some(LanguageSpec::new(raw));
        }
        ensure!(
            trailing.is_empty(),
            "Unexpected positional arguments after specifying --versions"
        );
        return Ok(Command::ShowVersions { language });
    }

    if let Some(pkg) = cli.install.as_ref() {
        let language = cli
            .lang
            .as_ref()
            .map(|value| LanguageSpec::new(value.to_string()));
        return Ok(Command::Install {
            language,
            package: pkg.clone(),
        });
    }

    // Apply --timeout if provided
    if let Some(secs) = cli.timeout {
        // SAFETY: called at startup before any threads are spawned
        unsafe { std::env::set_var("RUN_TIMEOUT_SECS", secs.to_string()) };
    }

    // Apply --timing if provided
    if cli.timing {
        // SAFETY: called at startup before any threads are spawned
        unsafe { std::env::set_var("RUN_TIMING", "1") };
    }

    if let Some(code) = cli.code.as_ref() {
        ensure!(
            !code.trim().is_empty(),
            "Inline code provided via --code must not be empty"
        );
    }

    let mut detect_language = !cli.no_detect;
    let mut trailing = cli.args.clone();
    let mut script_args: Vec<String> = Vec::new();

    let mut language = cli
        .lang
        .as_ref()
        .map(|value| LanguageSpec::new(value.to_string()));

    if language.is_none()
        && let Some(candidate) = trailing.first()
        && crate::language::is_language_token(candidate)
    {
        let raw = trailing.remove(0);
        language = Some(LanguageSpec::new(raw));
    }

    let mut source: Option<InputSource> = None;

    if let Some(code) = cli.code {
        ensure!(
            cli.file.is_none(),
            "--code/--inline cannot be combined with --file"
        );
        source = Some(InputSource::Inline(code));
        script_args = trailing;
        if script_args.first().map(|token| token.as_str()) == Some("--") {
            script_args.remove(0);
        }
        trailing = Vec::new();
    }

    if source.is_none()
        && let Some(path) = cli.file
    {
        source = Some(InputSource::File(path));
        script_args = trailing;
        if script_args.first().map(|token| token.as_str()) == Some("--") {
            script_args.remove(0);
        }
        trailing = Vec::new();
    }

    if source.is_none() && !trailing.is_empty() {
        match trailing.first().map(|token| token.as_str()) {
            Some("-c") | Some("--code") => {
                trailing.remove(0);
                let (code_tokens, extra_args) = split_at_double_dash(&trailing);
                ensure!(
                    !code_tokens.is_empty(),
                    "--code/--inline requires a code argument"
                );
                let joined = join_tokens(&code_tokens);
                source = Some(InputSource::Inline(joined));
                script_args = extra_args;
                trailing.clear();
            }
            Some("-f") | Some("--file") => {
                trailing.remove(0);
                ensure!(!trailing.is_empty(), "--file requires a path argument");
                let path = trailing.remove(0);
                source = Some(InputSource::File(PathBuf::from(path)));
                if trailing.first().map(|token| token.as_str()) == Some("--") {
                    trailing.remove(0);
                }
                script_args = trailing.clone();
                trailing.clear();
            }
            _ => {}
        }
    }

    if source.is_none() && !trailing.is_empty() {
        let first = trailing.remove(0);
        match first.as_str() {
            "-" => {
                source = Some(InputSource::Stdin);
                if trailing.first().map(|token| token.as_str()) == Some("--") {
                    trailing.remove(0);
                }
                script_args = trailing.clone();
                trailing.clear();
            }
            _ if looks_like_path(&first) => {
                source = Some(InputSource::File(PathBuf::from(first)));
                if trailing.first().map(|token| token.as_str()) == Some("--") {
                    trailing.remove(0);
                }
                script_args = trailing.clone();
                trailing.clear();
            }
            _ => {
                let mut all_tokens = Vec::with_capacity(trailing.len() + 1);
                all_tokens.push(first);
                all_tokens.extend(trailing.drain(..));
                let (code_tokens, extra_args) = split_at_double_dash(&all_tokens);
                let joined = join_tokens(&code_tokens);
                source = Some(InputSource::Inline(joined));
                script_args = extra_args;
            }
        }
    }

    if source.is_none() {
        let stdin = std::io::stdin();
        if !stdin.is_terminal() {
            source = Some(InputSource::Stdin);
        }
    }

    if language.is_some() && !cli.no_detect {
        detect_language = false;
    }

    if let Some(source) = source {
        let spec = ExecutionSpec {
            language,
            source,
            detect_language,
            args: script_args,
        };
        if let Some(n) = cli.bench {
            return Ok(Command::Bench {
                spec,
                iterations: n.max(1),
            });
        }
        if cli.watch {
            return Ok(Command::Watch { spec });
        }
        return Ok(Command::Execute(spec));
    }

    Ok(Command::Repl {
        initial_language: language,
        detect_language,
    })
}

#[derive(Parser, Debug)]
#[command(
    name = "run",
    about = "Universal multi-language runner and REPL",
    long_about = "Universal multi-language runner and REPL. Run 2.0 is available via 'run v2' and is experimental.",
    disable_help_subcommand = true,
    disable_version_flag = true
)]
struct Cli {
    #[arg(short = 'V', long = "version", action = clap::ArgAction::SetTrue)]
    version: bool,

    #[arg(
        short,
        long,
        value_name = "LANG",
        value_parser = NonEmptyStringValueParser::new()
    )]
    lang: Option<String>,

    #[arg(
        short,
        long,
        value_name = "PATH",
        value_hint = ValueHint::FilePath
    )]
    file: Option<PathBuf>,

    #[arg(
        short = 'c',
        long = "code",
        value_name = "CODE",
        value_parser = NonEmptyStringValueParser::new()
    )]
    code: Option<String>,

    #[arg(long = "no-detect", action = clap::ArgAction::SetTrue)]
    no_detect: bool,

    /// Maximum execution time in seconds (default: 60, override with RUN_TIMEOUT_SECS)
    #[arg(long = "timeout", value_name = "SECS")]
    timeout: Option<u64>,

    /// Show execution timing after each run
    #[arg(long = "timing", action = clap::ArgAction::SetTrue)]
    timing: bool,

    /// Check which language toolchains are available
    #[arg(long = "check", action = clap::ArgAction::SetTrue)]
    check: bool,

    /// Show toolchain versions for available languages
    #[arg(long = "versions", action = clap::ArgAction::SetTrue)]
    versions: bool,

    /// Install a package for a language (use -l to specify language, defaults to python)
    #[arg(long = "install", value_name = "PACKAGE")]
    install: Option<String>,

    /// Benchmark: run code N times and report min/max/avg timing
    #[arg(long = "bench", value_name = "N")]
    bench: Option<u32>,

    /// Watch a file and re-execute on changes
    #[arg(short = 'w', long = "watch", action = clap::ArgAction::SetTrue)]
    watch: bool,

    #[arg(value_name = "ARGS", trailing_var_arg = true)]
    args: Vec<String>,
}

fn join_tokens(tokens: &[String]) -> String {
    tokens.join(" ")
}

fn split_at_double_dash(tokens: &[String]) -> (Vec<String>, Vec<String>) {
    if let Some(index) = tokens.iter().position(|token| token == "--") {
        let before = tokens[..index].to_vec();
        let after = tokens[index + 1..].to_vec();
        (before, after)
    } else {
        (tokens.to_vec(), Vec::new())
    }
}

fn looks_like_path(token: &str) -> bool {
    if token == "-" {
        return true;
    }

    let path = Path::new(token);

    if path.is_absolute() {
        return true;
    }

    if token.starts_with("./") || token.starts_with("../") || token.starts_with("~/") {
        return true;
    }

    if std::fs::metadata(path).is_ok() {
        return true;
    }

    if token.chars().any(|ch| ch.is_whitespace()) {
        return false;
    }

    if let Some(ext) = path.extension().and_then(|ext| ext.to_str()) {
        let ext_lower = ext.to_ascii_lowercase();
        if KNOWN_CODE_EXTENSIONS
            .iter()
            .any(|candidate| candidate == &ext_lower.as_str())
        {
            return true;
        }
    }

    false
}

const KNOWN_CODE_EXTENSIONS: &[&str] = &[
    "py", "pyw", "rs", "rlib", "go", "js", "mjs", "cjs", "ts", "tsx", "jsx", "rb", "lua", "sh",
    "bash", "zsh", "ps1", "php", "java", "kt", "swift", "scala", "clj", "fs", "cs", "c", "cc",
    "cpp", "h", "hpp", "pl", "jl", "ex", "exs", "ml", "hs",
];
