use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
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
    pub json: bool,
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
    WatchFile {
        path: PathBuf,
        language: Option<LanguageSpec>,
        args: Vec<String>,
    },
    Format {
        path: PathBuf,
    },
    Snippet {
        language: LanguageSpec,
        name: Option<String>,
        list: bool,
    },
    Doctor,
    Cache {
        action: CacheAction,
    },
    Alias {
        action: AliasAction,
    },
    Share {
        path: PathBuf,
        port: Option<u16>,
    },
    PerfReport,
    PerfReset,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheAction {
    Stats,
    Clear,
    ClearLang(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AliasAction {
    List,
}

pub fn parse() -> Result<Command> {
    let cli = Cli::parse();

    if cli.version {
        return Ok(Command::ShowVersion);
    }
    if cli.perf_report {
        return Ok(Command::PerfReport);
    }
    if cli.perf_reset {
        return Ok(Command::PerfReset);
    }
    if cli.check {
        return Ok(Command::Doctor);
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

    crate::runtime::set_timeout(cli.timeout);

    if cli.timing {
        crate::runtime::enable_timing();
    }

    if let Some(code) = cli.code.as_ref() {
        ensure!(
            !code.trim().is_empty(),
            "Inline code provided via --code must not be empty"
        );
    }

    let mut trailing = cli.args.clone();
    if let Some(command) = parse_subcommand(&mut trailing, cli.lang.as_deref())? {
        return Ok(command);
    }

    let mut detect_language = !cli.no_detect;
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
                all_tokens.append(&mut trailing);
                let (code_tokens, extra_args) = split_at_double_dash(&all_tokens);
                let joined = join_tokens(&code_tokens);
                source = Some(InputSource::Inline(joined));
                script_args = extra_args;
            }
        }
    }

    if source.is_none() && !cli.interactive {
        let stdin = std::io::stdin();
        if !stdin.is_terminal() {
            source = Some(InputSource::Stdin);
        }
    }

    if cli.interactive {
        return Ok(Command::Repl {
            initial_language: language,
            detect_language,
        });
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
            json: cli.json,
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
    after_help = SUBCOMMAND_HELP,
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

    /// Maximum execution time in seconds (0 = unlimited, override with RUN_TIMEOUT_SECS)
    #[arg(long = "timeout", value_name = "SECS")]
    timeout: Option<u64>,

    /// Show execution timing after each run
    #[arg(long = "timing", action = clap::ArgAction::SetTrue)]
    timing: bool,

    /// Emit a machine-readable JSON envelope for one-shot execution
    #[arg(long = "json", action = clap::ArgAction::SetTrue)]
    json: bool,

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

    /// Show in-memory performance counters collected in this process
    #[arg(long = "perf-report", action = clap::ArgAction::SetTrue)]
    perf_report: bool,

    /// Reset in-memory performance counters in this process
    #[arg(long = "perf-reset", action = clap::ArgAction::SetTrue)]
    perf_reset: bool,

    /// Force REPL (interactive) mode even when stdin is not a TTY (e.g. piped input)
    #[arg(short = 'i', long = "interactive", action = clap::ArgAction::SetTrue)]
    interactive: bool,

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

fn parse_subcommand(args: &mut Vec<String>, lang: Option<&str>) -> Result<Option<Command>> {
    let Some(first) = args.first().map(String::as_str) else {
        return Ok(None);
    };

    match first {
        "doctor" => {
            args.remove(0);
            ensure!(
                args.is_empty(),
                "doctor does not accept positional arguments"
            );
            Ok(Some(Command::Doctor))
        }
        "fmt" => {
            args.remove(0);
            ensure!(!args.is_empty(), "fmt requires a file path");
            let path = PathBuf::from(args.remove(0));
            ensure!(args.is_empty(), "fmt accepts exactly one file path");
            Ok(Some(Command::Format { path }))
        }
        "snippet" => {
            args.remove(0);
            ensure!(!args.is_empty(), "snippet requires a language");
            let language = LanguageSpec::new(args.remove(0));
            let list = args
                .first()
                .is_some_and(|arg| arg == "--list" || arg == "-l");
            let name = if list {
                args.remove(0);
                None
            } else {
                args.first().cloned()
            };
            if name.is_some() {
                args.remove(0);
            }
            ensure!(
                args.is_empty(),
                "unexpected arguments after snippet command"
            );
            Ok(Some(Command::Snippet {
                language,
                name,
                list,
            }))
        }
        "cache" => {
            args.remove(0);
            let action = match args.first().map(String::as_str) {
                None | Some("--stats") | Some("stats") => {
                    if !args.is_empty() {
                        args.remove(0);
                    }
                    CacheAction::Stats
                }
                Some("--clear") | Some("clear") => {
                    args.remove(0);
                    CacheAction::Clear
                }
                Some("--clear-lang") | Some("clear-lang") => {
                    args.remove(0);
                    ensure!(!args.is_empty(), "cache --clear-lang requires a language");
                    CacheAction::ClearLang(args.remove(0))
                }
                Some(other) => anyhow::bail!("unknown cache action '{other}'"),
            };
            ensure!(args.is_empty(), "unexpected arguments after cache command");
            Ok(Some(Command::Cache { action }))
        }
        "alias" | "aliases" => {
            args.remove(0);
            let action = match args.first().map(String::as_str) {
                None | Some("list") | Some("--list") => {
                    if !args.is_empty() {
                        args.remove(0);
                    }
                    AliasAction::List
                }
                Some("add") | Some("set") | Some("remove") | Some("rm") | Some("delete") => {
                    anyhow::bail!(
                        "custom language aliases are not supported yet; use `run alias list` to view built-in aliases"
                    )
                }
                Some(other) => anyhow::bail!("unknown alias action '{other}'"),
            };
            ensure!(args.is_empty(), "unexpected arguments after alias command");
            Ok(Some(Command::Alias { action }))
        }
        "watch" => {
            args.remove(0);
            ensure!(!args.is_empty(), "watch requires a file path");
            let path = PathBuf::from(args.remove(0));
            let mut rest = std::mem::take(args);
            if rest.first().map(|token| token.as_str()) == Some("--") {
                rest.remove(0);
            }
            Ok(Some(Command::WatchFile {
                path,
                language: lang.map(|value| LanguageSpec::new(value.to_string())),
                args: rest,
            }))
        }
        "share" => {
            args.remove(0);
            let mut port = None;
            let mut path = None;
            while let Some(arg) = args.first().cloned() {
                args.remove(0);
                if arg == "--port" {
                    ensure!(!args.is_empty(), "share --port requires a port");
                    let value = args.remove(0);
                    port = Some(value.parse::<u16>()?);
                } else if path.is_none() {
                    path = Some(PathBuf::from(arg));
                } else {
                    anyhow::bail!("share accepts exactly one file path");
                }
            }
            let path = path.context("share requires a file path")?;
            Ok(Some(Command::Share { path, port }))
        }
        _ => Ok(None),
    }
}

fn looks_like_path(token: &str) -> bool {
    if token == "-" {
        return true;
    }

    if token.starts_with('-') || token.starts_with('"') || token.starts_with('\'') {
        return false;
    }

    let path = Path::new(token);

    if path.is_absolute() {
        return true;
    }

    if token.starts_with("./") || token.starts_with("../") || token.starts_with("~/") {
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

    if token.contains(std::path::MAIN_SEPARATOR) || token.contains('/') || token.contains('\\') {
        return std::fs::metadata(path).is_ok();
    }

    false
}

const KNOWN_CODE_EXTENSIONS: &[&str] = &[
    "py", "pyw", "rs", "rlib", "go", "js", "mjs", "cjs", "ts", "tsx", "jsx", "rb", "lua", "sh",
    "bash", "zsh", "ps1", "php", "java", "kt", "swift", "scala", "clj", "fs", "cs", "c", "cc",
    "cpp", "h", "hpp", "pl", "jl", "ex", "exs", "ml", "hs",
];

const SUBCOMMAND_HELP: &str = "\
Workflow commands:
  run doctor                 Diagnose installed language toolchains
  run cache --stats          Show persistent build cache usage
  run cache --clear          Clear all persistent build cache entries
  run cache --clear-lang L   Clear cache entries for one language
  run alias list             List built-in language aliases
  run fmt <file>             Format a file in place
  run snippet <lang> <name>  Print a curated offline snippet template
  run snippet <lang> --list  List templates for a language
  run watch <file>           Re-run a file when it changes
  run share <file> [--port N] Serve a local highlighted file/output page
  run v2 ...                 Use the experimental WASI component runtime";
