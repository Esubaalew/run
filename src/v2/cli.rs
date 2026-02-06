//! CLI Commands
//!
//! Commands: dev, exec, install, build, test, deploy, init, verify, clean, compose

use crate::v2::bridge::compose::{analyze_compose, migrate_compose_to_run};
use crate::v2::build::build_all;
use crate::v2::config::RunConfig;
use crate::v2::deploy::{DeployOptions, run_deploy};
use crate::v2::dev::{DevOptions, run_dev};
use crate::v2::plugins::{PluginHook, PluginManager};
use crate::v2::registry::{InstallOptions, Registry, RegistryConfig};
use crate::v2::test::{TestOptions, run_tests};
use crate::v2::toolchain::ToolchainManager;
use crate::v2::{Error, Result};
use std::path::PathBuf;

#[derive(Debug)]
pub enum V2Command {
    Dev {
        port: Option<u16>,
        no_hot_reload: bool,
        verbose: bool,
    },

    Install {
        package: Option<String>,
        version: Option<String>,
        dev: bool,
        features: Vec<String>,
    },

    Build {
        release: bool,
        reproducible: bool,
        component: Option<String>,
    },

    Exec {
        target: Option<String>,
        function: Option<String>,
        args: Vec<String>,

        allow_clock: bool,

        allow_random: bool,

        json: bool,
    },

    Init {
        name: String,
    },

    Info {
        package: Option<String>,
        components: bool,
        verbose: bool,
    },

    Update {
        package: Option<String>,
    },

    Clean {
        cache: bool,
    },

    Verify {
        offline: bool,
    },

    Test {
        component: Option<String>,
        build: bool,
        json: bool,
    },

    Deploy {
        target: Option<String>,
        profile: Option<String>,
        output: Option<String>,
        component: Option<String>,
        build: bool,
        registry_url: Option<String>,
        token: Option<String>,
        provider: Option<String>,
    },

    /// Publish to registry (alias for `deploy --target registry`)
    Publish {
        component: Option<String>,
        build: bool,
        registry_url: Option<String>,
        token: Option<String>,
    },

    Compose {
        action: ComposeAction,
        input: PathBuf,
        output: Option<PathBuf>,
    },

    Toolchain {
        action: ToolchainAction,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposeAction {
    Analyze,
    Migrate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolchainAction {
    Sync,
}

pub async fn execute(cmd: V2Command, project_dir: PathBuf) -> Result<i32> {
    match cmd {
        V2Command::Dev {
            port,
            no_hot_reload,
            verbose,
        } => {
            let config_path = project_dir.join("run.toml");
            if let Ok(config) = RunConfig::load(&config_path) {
                if let Ok(plugins) = PluginManager::load_all(&config, &project_dir).await {
                    let _ = plugins.run_hook(PluginHook::DevStart);
                }
            }
            let options = DevOptions {
                project_dir,
                port: port.unwrap_or(3000),
                hot_reload: !no_hot_reload,
                verbose,
                ..Default::default()
            };
            run_dev(options).await?;
            Ok(0)
        }

        V2Command::Install {
            package,
            version,
            dev,
            features,
        } => {
            let mut registry_config = RegistryConfig::default();
            let config_path = project_dir.join("run.toml");
            let mut run_config = if config_path.exists() {
                RunConfig::load(&config_path)?
            } else {
                let mut cfg = RunConfig::default();
                if let Some(name) = project_dir.file_name().and_then(|n| n.to_str()) {
                    cfg.project.name = name.to_string();
                }
                cfg
            };

            registry_config.registry_url = run_config.registry.url.clone();
            registry_config.mirrors = run_config.registry.mirrors.clone();
            registry_config.auth_token = run_config.registry.auth_token.clone();
            if let Ok(plugins) = PluginManager::load_all(&run_config, &project_dir).await {
                let _ = plugins.run_hook(PluginHook::Install);
            }

            let mut registry = Registry::new(registry_config, &project_dir)?;
            registry.load_lockfile()?;

            let options = InstallOptions {
                dev,
                ..Default::default()
            };
            if let Some(pkg) = package {
                registry.install(&pkg, version.as_deref(), options).await?;
                apply_dependency_update(&mut run_config, &pkg, version.as_deref(), dev, &features);
                run_config.save(&config_path)?;
                println!("Installed {}", pkg);
                Ok(0)
            } else {
                if !config_path.exists() {
                    return Err(Error::other(
                        "run.toml not found. Create one with `run init` or specify a package.",
                    ));
                }
                registry.install_all(options).await?;
                println!("Installed dependencies from run.toml");
                Ok(0)
            }
        }

        V2Command::Build {
            release,
            reproducible,
            component,
        } => {
            let config_path = project_dir.join("run.toml");
            let mut config = RunConfig::load(&config_path)?;

            // Verify toolchains if lockfile exists
            if reproducible {
                let toolchain_mgr = ToolchainManager::new(&project_dir)?;
                let required = ["cargo-component", "componentize-py", "wasm-tools"];
                for tool in &required {
                    if let Err(e) = toolchain_mgr.ensure_toolchain(tool) {
                        eprintln!("Warning: {}", e);
                    }
                }
            }

            if release {
                config.build.opt_level = "release".to_string();
            }
            if reproducible {
                config.build.reproducible = true;
            }
            if let Ok(plugins) = PluginManager::load_all(&config, &project_dir).await {
                let _ = plugins.run_hook(PluginHook::Build);
            }

            if let Some(ref comp_name) = component {
                if !config.components.contains_key(comp_name) {
                    return Err(Error::ComponentNotFound(comp_name.clone()));
                }
                config.components.retain(|k, _| k == comp_name);
            }

            println!("Building {} components...", config.components.len());

            let results = build_all(&config, &project_dir)?;
            for result in results {
                println!("[{}] built: {}", result.name, result.output_path.display());
            }

            println!("\nBuild complete");
            Ok(0)
        }

        V2Command::Exec {
            target,
            function,
            args,
            allow_clock,
            allow_random,
            json,
        } => {
            use crate::v2::runtime::{
                Capability, CapabilitySet, CliContext, ComponentValue, RuntimeConfig, RuntimeEngine,
            };

            let runtime_config = RuntimeConfig::production();
            let mut engine = RuntimeEngine::new(runtime_config)?;

            // Pre-load installed dependencies so cross-component imports resolve.
            let deps_dir = project_dir.join(".run").join("components");
            if deps_dir.is_dir() {
                if let Ok(entries) = std::fs::read_dir(&deps_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().map(|e| e == "wasm").unwrap_or(false) {
                            let dep_name = path
                                .file_stem()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_default();
                            let base_name = dep_name
                                .rsplit_once('@')
                                .map(|(n, _)| n.to_string())
                                .unwrap_or_else(|| dep_name.clone());
                            if let Ok(bytes) = std::fs::read(&path) {
                                let _ = engine.load_component_bytes(&base_name, bytes);
                            }
                        }
                    }
                }
            }

            let mut caps = CapabilitySet::deterministic();
            caps.grant(Capability::Stdout);
            caps.grant(Capability::Stderr);
            caps.grant(Capability::Args);

            if allow_clock {
                if !json {
                    eprintln!("WARNING: --allow-clock breaks determinism");
                }
                caps.grant(Capability::Clock);
            }
            if allow_random {
                if !json {
                    eprintln!("WARNING: --allow-random breaks determinism");
                }
                caps.grant(Capability::Random);
            }

            if !json {
                println!(
                    "[exec] mode=production clock={} random={}",
                    allow_clock, allow_random
                );
            }

            let target = target
                .ok_or_else(|| Error::other("Missing target. Usage: run exec <component|path>"))?;

            let wasm_path = {
                let candidate = project_dir.join(&target);
                if candidate.exists() && candidate.is_file() {
                    candidate
                } else if std::path::Path::new(&target).exists()
                    && std::path::Path::new(&target).is_file()
                {
                    PathBuf::from(&target)
                } else {
                    let config_path = project_dir.join("run.toml");
                    let config = RunConfig::load(&config_path)?;
                    let comp = config
                        .components
                        .get(&target)
                        .ok_or_else(|| Error::ComponentNotFound(target.clone()))?;
                    if let Some(ref path) = comp.path {
                        project_dir.join(path)
                    } else if let Some(ref source) = comp.source {
                        let source_path = project_dir.join(source);
                        if source_path
                            .extension()
                            .map(|e| e == "wasm")
                            .unwrap_or(false)
                        {
                            source_path
                        } else {
                            project_dir
                                .join(&config.build.output_dir)
                                .join(format!("{}.wasm", target))
                        }
                    } else {
                        project_dir
                            .join(&config.build.output_dir)
                            .join(format!("{}.wasm", target))
                    }
                }
            };

            if !wasm_path.exists() {
                return Err(Error::other(format!(
                    "Component not found at {}",
                    wasm_path.display()
                )));
            }

            if !json {
                println!("[exec] running: {}", wasm_path.display());
            }

            if function.is_none() && !args.is_empty() {
                return Err(Error::other("--args requires --function"));
            }

            let parsed_args = args
                .iter()
                .filter(|s| !s.trim().is_empty())
                .map(|s| ComponentValue::parse(s))
                .collect::<Result<Vec<_>>>()?;

            if let Some(func) = function {
                let component_id = engine.load_component(&wasm_path)?;
                let handle = engine.instantiate(&component_id, caps)?;
                let result = engine.call(&handle, &func, parsed_args)?;
                if json {
                    let output = serde_json::json!({
                        "exit_code": result.exit_code,
                        "stdout": String::from_utf8_lossy(&result.stdout),
                        "stderr": String::from_utf8_lossy(&result.stderr),
                        "return_value": result
                            .return_value
                            .as_ref()
                            .map(|v| v.to_json_value())
                            .unwrap_or(serde_json::Value::Null),
                    });
                    println!("{}", output);
                } else {
                    if !result.stdout.is_empty() {
                        print!("{}", String::from_utf8_lossy(&result.stdout));
                    }
                    if !result.stderr.is_empty() {
                        eprint!("{}", String::from_utf8_lossy(&result.stderr));
                    }
                    if let Some(value) = result.return_value.as_ref() {
                        println!("[exec] return: {}", value.to_display_string());
                    }
                    println!("[exec] completed (exit={})", result.exit_code);
                }
                Ok(result.exit_code)
            } else {
                let component_id = engine.load_component(&wasm_path)?;
                let handle = engine.instantiate(&component_id, caps)?;
                let ctx = CliContext {
                    args: vec![target.clone()],
                    env: vec![],
                    stdin: None,
                    cwd: std::env::current_dir().ok(),
                };
                let instance = engine
                    .get_instance(&handle)
                    .ok_or_else(|| Error::ComponentNotFound(component_id.clone()))?;
                let call = instance.run_cli(ctx)?;
                let result = crate::v2::runtime::ExecutionResult {
                    exit_code: call.exit_code,
                    stdout: call.stdout,
                    stderr: call.stderr,
                    duration_ms: 0,
                    return_value: call.return_value,
                };
                if json {
                    let output = serde_json::json!({
                        "exit_code": result.exit_code,
                        "stdout": String::from_utf8_lossy(&result.stdout),
                        "stderr": String::from_utf8_lossy(&result.stderr),
                        "return_value": result
                            .return_value
                            .as_ref()
                            .map(|v| v.to_json_value())
                            .unwrap_or(serde_json::Value::Null),
                    });
                    println!("{}", output);
                } else {
                    if !result.stdout.is_empty() {
                        print!("{}", String::from_utf8_lossy(&result.stdout));
                    }
                    if !result.stderr.is_empty() {
                        eprint!("{}", String::from_utf8_lossy(&result.stderr));
                    }
                    if let Some(value) = result.return_value.as_ref() {
                        println!("[exec] return: {}", value.to_display_string());
                    }
                    println!("[exec] completed (exit={})", result.exit_code);
                }
                Ok(result.exit_code)
            }
        }

        V2Command::Init { name } => {
            let config = RunConfig {
                project: crate::v2::config::ProjectConfig {
                    name: name.clone(),
                    version: "0.1.0".to_string(),
                    description: Some(format!("{} - A Run 2.0 project", name)),
                    authors: vec![],
                    license: Some("MIT".to_string()),
                    repository: None,
                },
                ..Default::default()
            };

            let config_path = project_dir.join("run.toml");
            config.save(&config_path)?;

            std::fs::create_dir_all(project_dir.join("components"))?;
            std::fs::create_dir_all(project_dir.join("wit"))?;

            // Generate a starter hello-world component.
            let hello_wat = format!(
                r#";; {name} - starter WASI component
;; Run with: run v2 exec components/hello.wasm --function greet --args "s32:42"
(component
  (core module $m
    (func (export "greet") (param i32) (result i32)
      local.get 0
      i32.const 1
      i32.add)
  )
  (core instance $i (instantiate $m))
  (type $greet_t (func (param "n" s32) (result s32)))
  (func $greet (type $greet_t) (canon lift (core func $i "greet")))
  (export "greet" (func $greet))
)
"#,
                name = name
            );
            std::fs::write(project_dir.join("components").join("hello.wat"), &hello_wat)?;

            // Generate a starter WIT interface.
            let wit_content = format!(
                r#"package {name}:hello@0.1.0;

interface greeter {{
    greet: func(n: s32) -> s32;
}}

world hello-world {{
    export greeter;
}}
"#,
                name = name
            );
            std::fs::write(project_dir.join("wit").join("hello.wit"), wit_content)?;

            // Append component to run.toml
            let mut toml_content = std::fs::read_to_string(&config_path)?;
            toml_content.push_str(&format!(
                "\n[components.hello]\npath = \"components/hello.wasm\"\n\n\
                 [tests.greet_42]\ncomponent = \"hello\"\nfunction = \"greet\"\n\
                 args = [\"s32:42\"]\nexpect = \"s32:43\"\n"
            ));
            std::fs::write(&config_path, toml_content)?;

            println!("Initialized Run 2.0 project: {}", name);
            println!("\nCreated:");
            println!("  run.toml              Project configuration");
            println!("  components/hello.wat  Starter WASI component (WAT source)");
            println!("  wit/hello.wit         WIT interface definition");
            println!("\nQuick start:");
            println!("  1. wasm-tools parse components/hello.wat -o components/hello.wasm");
            println!("  2. run v2 exec components/hello.wasm --function greet --args \"s32:42\"");
            println!("  3. run v2 test");
            println!("  4. run v2 dev");
            Ok(0)
        }

        V2Command::Info {
            package,
            components,
            verbose,
        } => {
            if let Some(ref pkg) = package {
                // Check if it's a local .wasm file first.
                let wasm_path = project_dir.join(pkg);
                if wasm_path.exists() && wasm_path.extension().map(|e| e == "wasm").unwrap_or(false)
                {
                    use crate::v2::runtime::{RuntimeConfig, RuntimeEngine};

                    let bytes = std::fs::read(&wasm_path)?;
                    let is_component = bytes.len() >= 8 && bytes[4..8] == [0x0d, 0x00, 0x01, 0x00];

                    println!("File: {}", wasm_path.display());
                    println!("Size: {} bytes", bytes.len());
                    println!(
                        "Type: {}",
                        if is_component {
                            "WASI component"
                        } else {
                            "WASM module"
                        }
                    );

                    let mut engine = RuntimeEngine::new(RuntimeConfig::default())?;
                    let comp_id = wasm_path
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "unknown".into());
                    if let Ok(id) = engine.load_component_bytes(&comp_id, bytes) {
                        if let Some(info) = engine.get_loaded_component(&id) {
                            if !info.exports.is_empty() {
                                println!("\nExports:");
                                for e in &info.exports {
                                    println!("  - {}", e);
                                }
                            }
                        }
                    }

                    return Ok(0);
                }

                let config = RegistryConfig::default();
                let registry = Registry::new(config, &project_dir)?;

                let info = registry.info(pkg).await?;
                println!("Package: {}", info.name);
                println!("Version: {}", info.version);
                println!("Description: {}", info.description);
                if let Some(license) = info.license {
                    println!("License: {}", license);
                }
                println!("Size: {} bytes", info.size);
                Ok(0)
            } else {
                let config_path = project_dir.join("run.toml");
                if !config_path.exists() {
                    return Err(Error::other("run.toml not found in current directory"));
                }
                let config = RunConfig::load(&config_path)?;
                println!(
                    "Project: {}@{}",
                    config.project.name, config.project.version
                );
                if let Some(ref desc) = config.project.description {
                    println!("Description: {}", desc);
                }
                println!("Components: {}", config.components.len());
                println!("Dependencies: {}", config.dependencies.len());
                if !config.dev_dependencies.is_empty() {
                    println!("Dev Dependencies: {}", config.dev_dependencies.len());
                }
                println!("Registry: {}", config.registry.url);

                if components || verbose {
                    println!("\nComponents:");
                    for (name, comp) in &config.components {
                        let mut details = Vec::new();
                        if let Some(ref path) = comp.path {
                            details.push(format!("path={}", path));
                        }
                        if let Some(ref source) = comp.source {
                            details.push(format!("source={}", source));
                        }
                        if let Some(ref lang) = comp.language {
                            details.push(format!("lang={}", lang));
                        }
                        if !comp.capabilities.is_empty() && verbose {
                            details.push(format!("caps={}", comp.capabilities.join(",")));
                        }
                        if details.is_empty() {
                            println!("  - {}", name);
                        } else {
                            println!("  - {} ({})", name, details.join(" "));
                        }
                    }
                }

                if verbose && !config.dependencies.is_empty() {
                    println!("\nDependencies:");
                    for (name, dep) in &config.dependencies {
                        if dep.features.is_empty() {
                            println!("  - {} {}", name, dep.version);
                        } else {
                            println!(
                                "  - {} {} [features: {}]",
                                name,
                                dep.version,
                                dep.features.join(",")
                            );
                        }
                    }
                }
                if verbose && !config.dev_dependencies.is_empty() {
                    println!("\nDev Dependencies:");
                    for (name, dep) in &config.dev_dependencies {
                        if dep.features.is_empty() {
                            println!("  - {} {}", name, dep.version);
                        } else {
                            println!(
                                "  - {} {} [features: {}]",
                                name,
                                dep.version,
                                dep.features.join(",")
                            );
                        }
                    }
                }
                Ok(0)
            }
        }

        V2Command::Update { package } => {
            let config = RegistryConfig::default();
            let mut registry = Registry::new(config, &project_dir)?;
            registry.load_lockfile()?;

            if let Some(pkg) = package {
                let new_version = registry.update(&pkg).await?;
                println!("Updated {} to {}", pkg, new_version);
            } else {
                let updates = registry.update_all().await?;
                if updates.is_empty() {
                    println!("All packages are up to date");
                } else {
                    for (name, version) in updates {
                        println!("Updated {} to {}", name, version);
                    }
                }
            }
            Ok(0)
        }

        V2Command::Clean { cache } => {
            if cache {
                let cache_dir = project_dir.join(".run").join("cache");
                if cache_dir.exists() {
                    std::fs::remove_dir_all(&cache_dir)?;
                    println!("Cleaned cache");
                }
            }

            let build_dir = project_dir.join("target").join("wasm");
            if build_dir.exists() {
                std::fs::remove_dir_all(&build_dir)?;
                println!("Cleaned build artifacts");
            }

            Ok(0)
        }

        V2Command::Verify { offline } => {
            use crate::v2::registry::{Lockfile, compute_sha256};

            let lockfile_path = project_dir.join("run.lock");
            if !lockfile_path.exists() {
                println!("No lockfile found. Run `run install` first.");
                return Ok(1);
            }

            let lockfile = Lockfile::load(&lockfile_path)?;
            let cache_dir = project_dir.join(".run").join("cache");
            let components: Vec<_> = lockfile.components().collect();

            let mut all_valid = true;
            let mut verified = 0;
            let mut failed = 0;

            println!("Verifying {} components...\n", components.len());

            for component in &components {
                let sanitized_name = component.name.replace(':', "__").replace('/', "_");
                let wasm_path =
                    cache_dir.join(format!("{}@{}.wasm", sanitized_name, component.version));

                if !wasm_path.exists() {
                    if offline {
                        println!("  MISSING  {}@{}", component.name, component.version);
                        all_valid = false;
                        failed += 1;
                        continue;
                    } else {
                        println!(
                            "  MISSING  {}@{} (not cached)",
                            component.name, component.version
                        );
                        all_valid = false;
                        failed += 1;
                        continue;
                    }
                }

                let bytes = std::fs::read(&wasm_path)?;
                let actual_hash = compute_sha256(&bytes);

                if actual_hash == component.sha256 {
                    println!("  OK       {}@{}", component.name, component.version);
                    verified += 1;
                } else {
                    println!("  CORRUPTED {}@{}", component.name, component.version);
                    println!("           expected: {}", component.sha256);
                    println!("           actual:   {}", actual_hash);
                    all_valid = false;
                    failed += 1;
                }
            }

            println!();
            if all_valid {
                println!("All {} components verified.", verified);
                Ok(0)
            } else {
                println!("{} verified, {} failed.", verified, failed);
                Ok(1)
            }
        }

        V2Command::Test {
            component,
            build,
            json,
        } => {
            let config_path = project_dir.join("run.toml");
            if let Ok(config) = RunConfig::load(&config_path) {
                if let Ok(plugins) = PluginManager::load_all(&config, &project_dir).await {
                    let _ = plugins.run_hook(PluginHook::Test);
                }
            }
            let options = TestOptions {
                project_dir,
                component,
                build,
                json,
            };
            let report = run_tests(options)?;
            if json {
                println!(
                    "{{\"total\":{},\"passed\":{},\"failed\":{}}}",
                    report.total, report.passed, report.failed
                );
            } else {
                println!("\n{} passed, {} failed", report.passed, report.failed);
            }
            Ok(if report.failed == 0 { 0 } else { 1 })
        }

        V2Command::Deploy {
            target,
            profile,
            output,
            component,
            build,
            registry_url,
            token,
            provider,
        } => {
            let config_path = project_dir.join("run.toml");
            let config = RunConfig::load(&config_path)?;
            if let Ok(plugins) = PluginManager::load_all(&config, &project_dir).await {
                let _ = plugins.run_hook(PluginHook::Deploy);
            }

            let options = DeployOptions {
                project_dir,
                target,
                profile,
                output_dir: output.map(PathBuf::from),
                component,
                build,
                registry_url,
                auth_token: token,
                provider,
            };
            run_deploy(options).await?;
            Ok(0)
        }

        V2Command::Publish {
            component,
            build,
            registry_url,
            token,
        } => {
            let config_path = project_dir.join("run.toml");
            let config = RunConfig::load(&config_path)?;
            if let Ok(plugins) = PluginManager::load_all(&config, &project_dir).await {
                let _ = plugins.run_hook(PluginHook::Deploy);
            }

            let options = DeployOptions {
                project_dir,
                target: Some("registry".to_string()),
                profile: None,
                output_dir: None,
                component,
                build,
                registry_url,
                auth_token: token,
                provider: None,
            };
            run_deploy(options).await?;
            Ok(0)
        }

        V2Command::Compose {
            action,
            input,
            output,
        } => match action {
            ComposeAction::Analyze => {
                let analysis = analyze_compose(&input)?;
                println!("Services: {}", analysis.total);
                println!("WASI candidates: {}", analysis.wasm_components.len());
                println!("Docker services: {}", analysis.docker_services.len());
                if !analysis.wasm_components.is_empty() {
                    println!("\nWASI components:");
                    for name in analysis.wasm_components {
                        println!("  - {}", name);
                    }
                }
                if !analysis.docker_services.is_empty() {
                    println!("\nDocker services:");
                    for name in analysis.docker_services {
                        println!("  - {}", name);
                    }
                }
                Ok(0)
            }
            ComposeAction::Migrate => {
                let output_path = output.unwrap_or_else(|| PathBuf::from("run.toml"));
                migrate_compose_to_run(&input, &output_path)?;
                Ok(0)
            }
        },

        V2Command::Toolchain { action } => match action {
            ToolchainAction::Sync => {
                let mut toolchain_mgr = ToolchainManager::new(&project_dir)?;
                let lockfile = toolchain_mgr.sync()?;
                println!(
                    "Synced {} toolchains to run.lock.toml",
                    lockfile.toolchains.len()
                );
                Ok(0)
            }
        },
    }
}

fn apply_dependency_update(
    config: &mut RunConfig,
    name: &str,
    version: Option<&str>,
    dev: bool,
    features: &[String],
) {
    let target = if dev {
        &mut config.dev_dependencies
    } else {
        &mut config.dependencies
    };

    let dep =
        target
            .entry(name.to_string())
            .or_insert_with(|| crate::v2::config::DependencyConfig {
                version: "*".to_string(),
                optional: false,
                features: vec![],
                git: None,
                path: None,
            });
    if let Some(ver) = version {
        dep.version = ver.to_string();
    }
    if !features.is_empty() {
        dep.features = features.to_vec();
    }
}

pub fn print_help() {
    println!("Run 2.0 (Experimental) - WASI Universal Runtime\n");
    println!("USAGE:");
    println!("    run v2 <COMMAND> [OPTIONS]\n");
    println!("COMMANDS:");
    println!("    dev          Start development server (clock allowed, hot reload)");
    println!("    exec         Execute in production mode (strict determinism)");
    println!("    install      Install a WASI component");
    println!("    build        Build all components");
    println!("    test         Run component tests");
    println!("    deploy       Package and deploy components");
    println!("    publish      Publish component to registry");
    println!("    init         Initialize a new project");
    println!("    info         Show component info");
    println!("    update       Update dependencies");
    println!("    verify       Verify all components against lockfile");
    println!("    clean        Clean build artifacts and cache");
    println!("    compose      Analyze/migrate docker-compose.yml");
    println!("    toolchain    Sync toolchain lockfile\n");
    println!("MODE DIFFERENCES:");
    println!("    dev          clock=YES, random=NO, hot_reload=YES, limits=relaxed");
    println!("    exec         clock=NO, random=NO, hot_reload=NO, limits=enforced\n");
    println!("OPTIONS:");
    println!("    -h, --help       Print help");
    println!("    -v, --verbose    Verbose output");
    println!("    --version        Print version\n");
    println!("EXAMPLES:");
    println!("    run v2 dev                    # Start dev server");
    println!("    run v2 install wasi:http      # Install WASI HTTP component");
    println!("    run v2 install                # Install from run.toml");
    println!("    run v2 build --release        # Build for production");
    println!("    run v2 build --component api  # Build a specific component");
    println!("    run v2 build --reproducible   # Build with reproducible env");
    println!("    run v2 test --build           # Build then run tests");
    println!("    run v2 deploy --target local  # Package deployment bundle");
    println!("    run v2 publish --build        # Build and publish to registry");
    println!("    run v2 compose analyze docker-compose.yml");
    println!("    run v2 compose migrate docker-compose.yml run.toml");
    println!("    run v2 toolchain sync         # Update run.lock.toml");
}

pub fn print_subcommand_help(subcommand: &str) {
    match subcommand {
        "dev" => {
            println!("run v2 dev - Start development server with hot reload\n");
            println!("USAGE: run v2 dev [OPTIONS]\n");
            println!("OPTIONS:");
            println!("    --port <PORT>       Dev server port (default: 3000)");
            println!("    --no-hot-reload     Disable file watching");
            println!("    -v, --verbose       Show inter-component calls\n");
            println!("EXAMPLES:");
            println!("    run v2 dev");
            println!("    run v2 dev --port 8080");
        }
        "exec" => {
            println!("run v2 exec - Execute a WASI component in production mode\n");
            println!("USAGE: run v2 exec <TARGET> [OPTIONS]\n");
            println!("ARGS:");
            println!(
                "    <TARGET>            Path to .wasm file or component name from run.toml\n"
            );
            println!("OPTIONS:");
            println!("    --function <NAME>   Call a specific exported function");
            println!("    --args <VALUE>      Arguments (repeat or comma-separated)");
            println!("    --json              Structured JSON output");
            println!("    --allow-clock       Allow clock access (breaks determinism)");
            println!("    --allow-random      Allow random access (breaks determinism)\n");
            println!("EXAMPLES:");
            println!("    run v2 exec app.wasm");
            println!("    run v2 exec app.wasm --function add --args 3 --args 4");
            println!("    run v2 exec app.wasm --function add --args \"3,4\" --json");
        }
        "install" => {
            println!("run v2 install - Install WASI components from registry\n");
            println!("USAGE: run v2 install [PACKAGE[@VERSION]] [OPTIONS]\n");
            println!("ARGS:");
            println!("    [PACKAGE]           Package name (e.g. 'wasi:http' or 'ns:name@1.0.0')");
            println!(
                "                        If omitted, installs all dependencies from run.toml\n"
            );
            println!("OPTIONS:");
            println!("    --dev               Install as dev dependency");
            println!("    --features <LIST>   Comma-separated feature list\n");
            println!("EXAMPLES:");
            println!("    run v2 install wasi:http");
            println!("    run v2 install mylib@0.2.0 --dev");
            println!("    run v2 install                   # install from run.toml");
        }
        "build" => {
            println!("run v2 build - Build WASI 0.2 components from source\n");
            println!("USAGE: run v2 build [OPTIONS]\n");
            println!("OPTIONS:");
            println!("    --release           Optimized release build");
            println!("    --reproducible      Enable reproducible build environment");
            println!("    --component <NAME>  Build a single component\n");
            println!("SUPPORTED LANGUAGES: Rust, Go, Python, JavaScript, TypeScript, Zig\n");
            println!("EXAMPLES:");
            println!("    run v2 build");
            println!("    run v2 build --release --reproducible");
            println!("    run v2 build --component api");
        }
        "test" => {
            println!("run v2 test - Run component tests\n");
            println!("USAGE: run v2 test [OPTIONS]\n");
            println!("OPTIONS:");
            println!("    --build             Build components before testing");
            println!("    --component <NAME>  Test a specific component");
            println!("    --json              Structured JSON output\n");
            println!("Tests can be defined in [tests.<name>] sections of run.toml,");
            println!("or auto-discovered from component exports named test_*.\n");
            println!("EXAMPLES:");
            println!("    run v2 test");
            println!("    run v2 test --build --component api");
        }
        "publish" => {
            println!("run v2 publish - Publish a component to the registry\n");
            println!("USAGE: run v2 publish [OPTIONS]\n");
            println!("OPTIONS:");
            println!("    --build             Build before publishing");
            println!("    --token <TOKEN>     Auth token (or set RUN_REGISTRY_TOKEN)\n");
            println!("EXAMPLES:");
            println!("    run v2 publish");
            println!("    run v2 publish --build --token mytoken");
        }
        "deploy" => {
            println!("run v2 deploy - Deploy components to target environments\n");
            println!("USAGE: run v2 deploy [OPTIONS]\n");
            println!("OPTIONS:");
            println!("    --target <TARGET>   Deployment target: local, edge, registry");
            println!("    --build             Build before deploying\n");
            println!("EDGE PROVIDERS: cloudflare, fastly, aws-lambda, vercel\n");
            println!("EXAMPLES:");
            println!("    run v2 deploy --target local");
            println!("    run v2 deploy --target edge");
        }
        "init" => {
            println!("run v2 init - Initialize a new Run 2.0 project\n");
            println!("USAGE: run v2 init <NAME>\n");
            println!("EXAMPLES:");
            println!("    run v2 init my-app");
        }
        _ => print_help(),
    }
}
