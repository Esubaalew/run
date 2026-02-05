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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Execute(ExecutionSpec),
    Repl {
        initial_language: Option<LanguageSpec>,
        detect_language: bool,
    },
    ShowVersion,
}

pub fn parse() -> Result<Command> {
    let cli = Cli::parse();

    if cli.version {
        return Ok(Command::ShowVersion);
    }
    if let Some(code) = cli.code.as_ref() {
        ensure!(
            !code.trim().is_empty(),
            "Inline code provided via --code must not be empty"
        );
    }

    let mut detect_language = !cli.no_detect;
    let mut trailing = cli.args.clone();

    let mut language = cli
        .lang
        .as_ref()
        .map(|value| LanguageSpec::new(value.to_string()));

    if language.is_none() {
        if let Some(candidate) = trailing.first() {
            if crate::language::is_language_token(candidate) {
                let raw = trailing.remove(0);
                language = Some(LanguageSpec::new(raw));
            }
        }
    }

    let mut source: Option<InputSource> = None;

    if let Some(code) = cli.code {
        ensure!(
            cli.file.is_none(),
            "--code/--inline cannot be combined with --file"
        );
        ensure!(
            trailing.is_empty(),
            "Unexpected positional arguments after specifying --code"
        );
        source = Some(InputSource::Inline(code));
    }

    if source.is_none() {
        if let Some(path) = cli.file {
            ensure!(
                trailing.is_empty(),
                "Unexpected positional arguments when --file is present"
            );
            source = Some(InputSource::File(path));
        }
    }

    if source.is_none() && !trailing.is_empty() {
        match trailing.first().map(|token| token.as_str()) {
            Some("-c") | Some("--code") => {
                trailing.remove(0);
                ensure!(
                    !trailing.is_empty(),
                    "--code/--inline requires a code argument"
                );
                let joined = join_tokens(&trailing);
                source = Some(InputSource::Inline(joined));
                trailing.clear();
            }
            Some("-f") | Some("--file") => {
                trailing.remove(0);
                ensure!(!trailing.is_empty(), "--file requires a path argument");
                ensure!(
                    trailing.len() == 1,
                    "Unexpected positional arguments after specifying --file"
                );
                let path = trailing.remove(0);
                source = Some(InputSource::File(PathBuf::from(path)));
                trailing.clear();
            }
            _ => {}
        }
    }

    if source.is_none() && !trailing.is_empty() {
        if trailing.len() == 1 {
            let token = trailing.remove(0);
            match token.as_str() {
                "-" => {
                    source = Some(InputSource::Stdin);
                }
                _ if looks_like_path(&token) => {
                    source = Some(InputSource::File(PathBuf::from(token)));
                }
                _ => {
                    source = Some(InputSource::Inline(token));
                }
            }
        } else {
            let joined = join_tokens(&trailing);
            source = Some(InputSource::Inline(joined));
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
        return Ok(Command::Execute(ExecutionSpec {
            language,
            source,
            detect_language,
        }));
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

    #[arg(value_name = "ARGS", trailing_var_arg = true)]
    args: Vec<String>,
}

fn join_tokens(tokens: &[String]) -> String {
    tokens.join(" ")
}

fn looks_like_path(token: &str) -> bool {
    if token == "-" {
        return true;
    }

    let path = Path::new(token);

    if path.is_absolute() {
        return true;
    }

    if token.contains(std::path::MAIN_SEPARATOR) || token.contains('\\') {
        return true;
    }

    if token.starts_with("./") || token.starts_with("../") || token.starts_with("~/") {
        return true;
    }

    if std::fs::metadata(path).is_ok() {
        return true;
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
