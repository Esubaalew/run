use anyhow::Result;

#[cfg(feature = "v2")]
use std::path::PathBuf;
#[cfg(feature = "v2")]
use run::v2::cli::{V2Command, execute as execute_v2};

fn main() -> Result<()> {
    #[cfg(feature = "v2")]
    {
        let args: Vec<String> = std::env::args().collect();
        let v2_args: Vec<String> = if args.len() > 1 && args[1] == "v2" {
            args[2..].to_vec()
        } else {
            Vec::new()
        };

        if args.len() > 1 && args[1] == "v2" {
            if v2_args.is_empty() {
                run::v2::cli::print_help();
                std::process::exit(0);
            }

            let subcommand = &v2_args[0];
            let cwd = std::env::current_dir()?;
            let has_flag = |flag: &str| v2_args.iter().any(|a| a == flag);

            if has_flag("--help") || has_flag("-h") {
                run::v2::cli::print_subcommand_help(subcommand);
                std::process::exit(0);
            }

            let v2_cmd = match subcommand.as_str() {
                "dev" => {
                    let mut port = None;
                    let mut no_hot_reload = false;
                    let mut i = 1;
                    while i < v2_args.len() {
                        match v2_args[i].as_str() {
                            "--port" if i + 1 < v2_args.len() => {
                                if let Ok(parsed) = v2_args[i + 1].parse::<u16>() {
                                    port = Some(parsed);
                                }
                                i += 1;
                            }
                            "--no-hot-reload" => no_hot_reload = true,
                            _ => {}
                        }
                        i += 1;
                    }
                    Some(V2Command::Dev {
                        port,
                        no_hot_reload,
                        verbose: has_flag("--verbose") || has_flag("-v"),
                    })
                }
                "install" => {
                    let mut package: Option<String> = None;
                    let mut version: Option<String> = None;
                    let mut dev = false;
                    let mut features: Vec<String> = Vec::new();

                    let mut i = 1;
                    while i < v2_args.len() {
                        match v2_args[i].as_str() {
                            "--dev" => dev = true,
                            "--features" if i + 1 < v2_args.len() => {
                                features = v2_args[i + 1]
                                    .split(',')
                                    .map(|s| s.trim().to_string())
                                    .filter(|s| !s.is_empty())
                                    .collect();
                                i += 1;
                            }
                            _ if !v2_args[i].starts_with('-') && package.is_none() => {
                                let spec = &v2_args[i];
                                let (pkg, ver) = if let Some(at_pos) = spec.rfind('@') {
                                    let pkg = spec[..at_pos].to_string();
                                    let ver = spec[at_pos + 1..].to_string();
                                    (pkg, Some(ver))
                                } else {
                                    (spec.clone(), None)
                                };
                                package = Some(pkg);
                                version = ver;
                            }
                            _ => {}
                        }
                        i += 1;
                    }

                    Some(V2Command::Install {
                        package,
                        version,
                        dev,
                        features,
                    })
                }
                "build" => {
                    let mut component = None;
                    let mut i = 1;
                    while i < v2_args.len() {
                        match v2_args[i].as_str() {
                            "--component" if i + 1 < v2_args.len() => {
                                component = Some(v2_args[i + 1].clone());
                                i += 1;
                            }
                            _ => {}
                        }
                        i += 1;
                    }
                    Some(V2Command::Build {
                        release: has_flag("--release"),
                        reproducible: has_flag("--reproducible"),
                        component,
                    })
                }
                "init" => {
                    if v2_args.len() < 2 {
                        eprintln!("Usage: run v2 init <name>");
                        std::process::exit(1);
                    }
                    Some(V2Command::Init {
                        name: v2_args[1].clone(),
                    })
                }
                "exec" | "start" => {
                    let mut target: Option<String> = None;
                    let mut function: Option<String> = None;
                    let mut call_args: Vec<String> = Vec::new();
                    let mut i = 1;
                    while i < v2_args.len() {
                        match v2_args[i].as_str() {
                            "--function" if i + 1 < v2_args.len() => {
                                function = Some(v2_args[i + 1].clone());
                                i += 1;
                            }
                            "--args" if i + 1 < v2_args.len() => {
                                // Support both comma-separated (--args "3,4")
                                // and repeated (--args 3 --args 4) forms.
                                for part in v2_args[i + 1].split(',') {
                                    let trimmed = part.trim().to_string();
                                    if !trimmed.is_empty() {
                                        call_args.push(trimmed);
                                    }
                                }
                                i += 1;
                            }
                            _ if !v2_args[i].starts_with('-') && target.is_none() => {
                                target = Some(v2_args[i].clone());
                            }
                            _ => {}
                        }
                        i += 1;
                    }
                    Some(V2Command::Exec {
                        target,
                        function,
                        args: call_args,
                        allow_clock: has_flag("--allow-clock"),
                        allow_random: has_flag("--allow-random"),
                        json: has_flag("--json"),
                    })
                }
                "info" => {
                    let mut package = None;
                    let mut components = false;
                    let mut verbose = false;
                    let mut i = 1;
                    while i < v2_args.len() {
                        match v2_args[i].as_str() {
                            "--components" => components = true,
                            "--verbose" => verbose = true,
                            _ if !v2_args[i].starts_with('-') && package.is_none() => {
                                package = Some(v2_args[i].clone());
                            }
                            _ => {}
                        }
                        i += 1;
                    }
                    Some(V2Command::Info {
                        package,
                        components,
                        verbose,
                    })
                }
                "update" => Some(V2Command::Update {
                    package: v2_args.get(1).cloned(),
                }),
                "clean" => Some(V2Command::Clean {
                    cache: has_flag("--cache"),
                }),
                "verify" => Some(V2Command::Verify {
                    offline: has_flag("--offline"),
                }),
                "test" => {
                    let mut component = None;
                    let mut build = false;
                    let mut json = false;
                    let mut i = 1;
                    while i < v2_args.len() {
                        match v2_args[i].as_str() {
                            "--component" if i + 1 < v2_args.len() => {
                                component = Some(v2_args[i + 1].clone());
                                i += 1;
                            }
                            "--build" => build = true,
                            "--json" => json = true,
                            _ => {}
                        }
                        i += 1;
                    }
                    Some(V2Command::Test {
                        component,
                        build,
                        json,
                    })
                }
                "deploy" => {
                    let mut target = None;
                    let mut profile = None;
                    let mut output = None;
                    let mut component = None;
                    let mut build = false;
                    let mut registry_url = None;
                    let mut token = None;
                    let mut provider = None;
                    let mut i = 1;
                    while i < v2_args.len() {
                        match v2_args[i].as_str() {
                            "--target" if i + 1 < v2_args.len() => {
                                target = Some(v2_args[i + 1].clone());
                                i += 1;
                            }
                            "--profile" if i + 1 < v2_args.len() => {
                                profile = Some(v2_args[i + 1].clone());
                                i += 1;
                            }
                            "--provider" if i + 1 < v2_args.len() => {
                                provider = Some(v2_args[i + 1].clone());
                                i += 1;
                            }
                            "--output" if i + 1 < v2_args.len() => {
                                output = Some(v2_args[i + 1].clone());
                                i += 1;
                            }
                            "--component" if i + 1 < v2_args.len() => {
                                component = Some(v2_args[i + 1].clone());
                                i += 1;
                            }
                            "--registry-url" if i + 1 < v2_args.len() => {
                                registry_url = Some(v2_args[i + 1].clone());
                                i += 1;
                            }
                            "--token" if i + 1 < v2_args.len() => {
                                token = Some(v2_args[i + 1].clone());
                                i += 1;
                            }
                            "--build" => build = true,
                            _ => {}
                        }
                        i += 1;
                    }
                    Some(V2Command::Deploy {
                        target,
                        profile,
                        output,
                        component,
                        build,
                        registry_url,
                        token,
                        provider,
                    })
                }
                "publish" => {
                    let mut component = None;
                    let mut build = false;
                    let mut registry_url = None;
                    let mut token = None;
                    let mut i = 1;
                    while i < v2_args.len() {
                        match v2_args[i].as_str() {
                            "--component" if i + 1 < v2_args.len() => {
                                component = Some(v2_args[i + 1].clone());
                                i += 1;
                            }
                            "--registry-url" if i + 1 < v2_args.len() => {
                                registry_url = Some(v2_args[i + 1].clone());
                                i += 1;
                            }
                            "--token" if i + 1 < v2_args.len() => {
                                token = Some(v2_args[i + 1].clone());
                                i += 1;
                            }
                            "--build" => build = true,
                            _ => {}
                        }
                        i += 1;
                    }
                    Some(V2Command::Publish {
                        component,
                        build,
                        registry_url,
                        token,
                    })
                }
                "compose" => {
                    if v2_args.len() < 3 {
                        eprintln!(
                            "Usage: run v2 compose <analyze|migrate> <docker-compose.yml> [run.toml]"
                        );
                        std::process::exit(1);
                    }
                    let action = match v2_args[1].as_str() {
                        "analyze" => run::v2::cli::ComposeAction::Analyze,
                        "migrate" => run::v2::cli::ComposeAction::Migrate,
                        _ => {
                            eprintln!("Unknown compose action: {}", v2_args[1]);
                            std::process::exit(1);
                        }
                    };
                    let input = PathBuf::from(&v2_args[2]);
                    let output =
                        if action == run::v2::cli::ComposeAction::Migrate && v2_args.len() > 3 {
                            Some(PathBuf::from(&v2_args[3]))
                        } else {
                            None
                        };
                    Some(V2Command::Compose {
                        action,
                        input,
                        output,
                    })
                }
                "toolchain" => {
                    if v2_args.len() < 2 {
                        eprintln!("Usage: run v2 toolchain sync");
                        std::process::exit(1);
                    }
                    let action = match v2_args[1].as_str() {
                        "sync" => run::v2::cli::ToolchainAction::Sync,
                        _ => {
                            eprintln!("Unknown toolchain action: {}", v2_args[1]);
                            std::process::exit(1);
                        }
                    };
                    Some(V2Command::Toolchain { action })
                }
                "help" | "--help" | "-h" => {
                    run::v2::cli::print_help();
                    std::process::exit(0);
                }
                "version" | "--version" | "-V" => {
                    println!("{}", run::v2::version_info());
                    std::process::exit(0);
                }
                _ => None,
            };

            if let Some(cmd) = v2_cmd {
                let rt = tokio::runtime::Runtime::new()?;
                let exit_code = rt.block_on(execute_v2(cmd, cwd))?;
                std::process::exit(exit_code);
            }
        }
    }

    // Load project config (run.toml / .runrc) and apply environment overrides
    let config = run::config::RunConfig::discover();
    config.apply_env();

    let command = run::cli::parse()?;
    let exit_code = run::app::run(command)?;
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
    Ok(())
}
