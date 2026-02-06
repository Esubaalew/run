//! Development Server
//!
//! `run dev` - Fast local development with hot reload.

mod output;
mod server;
mod watcher;

pub use output::{OutputLine, OutputManager, OutputStyle};
pub use server::{DevServer, DevServerConfig, DevServerNotifier};
pub use watcher::{FileWatcher, WatchEvent};

use crate::v2::Result;
use crate::v2::bridge::{Bridge, BridgeConfig, ConnectionInfo, DockerConfig};
use crate::v2::build::build_all;
use crate::v2::config::RunConfig;
use crate::v2::orchestrator::{Orchestrator, OrchestratorConfig, OrchestratorEvent};
use crate::v2::runtime::{Capability, CapabilitySet, RuntimeConfig, RuntimeEngine};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;
#[derive(Debug, Clone)]
pub struct DevOptions {
    pub project_dir: PathBuf,

    pub port: u16,

    pub hot_reload: bool,

    pub verbose: bool,

    pub components: Vec<String>,

    pub env: Vec<(String, String)>,
}

impl Default for DevOptions {
    fn default() -> Self {
        Self {
            project_dir: PathBuf::from("."),
            port: 3000,
            hot_reload: true,
            verbose: false,
            components: vec![],
            env: vec![],
        }
    }
}
pub struct DevSession {
    config: RunConfig,

    project_dir: PathBuf,

    runtime: Arc<Mutex<RuntimeEngine>>,

    orchestrator: Arc<Orchestrator>,

    bridge: Option<Bridge>,

    watcher: Option<FileWatcher>,

    output: OutputManager,

    start_time: Instant,

    running: Arc<std::sync::atomic::AtomicBool>,

    server: Option<DevServer>,

    server_notifier: Option<DevServerNotifier>,
}

impl DevSession {
    pub fn new(options: DevOptions) -> Result<Self> {
        let start_time = Instant::now();

        let config_path = options.project_dir.join("run.toml");
        let has_config = config_path.exists();
        let mut config = if has_config {
            RunConfig::load(&config_path)?
        } else {
            RunConfig::default()
        };
        config.dev.port = options.port;

        let runtime_config = RuntimeConfig::development();
        let runtime = RuntimeEngine::new(runtime_config)?;
        let runtime = Arc::new(Mutex::new(runtime));

        let orch_config = OrchestratorConfig::default();
        let orchestrator = Arc::new(crate::v2::orchestrator::create_orchestrator(
            Arc::clone(&runtime),
            orch_config,
        ));

        let bridge = if !config.dev.services.is_empty() {
            Some(Bridge::new(BridgeConfig::default())?)
        } else {
            None
        };

        let output = OutputManager::new(options.verbose);

        let watcher = if options.hot_reload && has_config {
            let watch_patterns = config.dev.watch.clone();
            Some(FileWatcher::new(&options.project_dir, watch_patterns)?)
        } else {
            None
        };

        let orchestrator_for_status = Arc::clone(&orchestrator);
        let status_provider: Arc<dyn Fn() -> Vec<server::ComponentStatus> + Send + Sync> =
            Arc::new(move || {
                let statuses = orchestrator_for_status.all_statuses();
                let mut result = Vec::new();
                for (name, status) in statuses {
                    let metrics = orchestrator_for_status
                        .component_metrics(&name)
                        .unwrap_or_default();
                    result.push(server::ComponentStatus {
                        name,
                        running: status == crate::v2::orchestrator::ComponentStatus::Running,
                        call_count: metrics.call_count,
                        error_count: metrics.error_count,
                        uptime_ms: metrics.uptime_ms,
                    });
                }
                result
            });

        let server_config = DevServerConfig {
            port: config.dev.port,
            host: "127.0.0.1".to_string(),
            websocket: false,
            dashboard: true,
            project_name: config.project.name.clone(),
        };
        let server = DevServer::new(server_config, status_provider);
        let server_notifier = Some(server.notifier());

        Ok(Self {
            config,
            project_dir: options.project_dir,
            runtime,
            orchestrator,
            bridge,
            watcher,
            output,
            start_time,
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            server: Some(server),
            server_notifier,
        })
    }
    pub async fn start(&mut self) -> Result<()> {
        self.running
            .store(true, std::sync::atomic::Ordering::SeqCst);

        if !self.project_dir.join("run.toml").exists() {
            self.output
                .log_warning("config", "run.toml not found; hot reload disabled");
        }

        let startup_time = self.start_time.elapsed();
        self.output
            .print_banner(&self.config.project.name, startup_time);

        let needs_build = self
            .config
            .components
            .values()
            .any(|comp| comp.path.is_none() && (comp.source.is_some() || comp.build.is_some()));
        if needs_build {
            self.output.log_system("Building components...");
            if let Err(e) = build_all(&self.config, &self.project_dir) {
                self.output
                    .log_error("build", &format!("build failed: {}", e));
            }
        }

        if let Some(ref mut bridge) = self.bridge {
            for service in &self.config.dev.services {
                let service_lower = service.to_lowercase();
                let docker_config = match service_lower.as_str() {
                    "postgres" | "postgresql" => DockerConfig::postgres("secret"),
                    "redis" => DockerConfig::redis(),
                    "mysql" => DockerConfig::mysql("root"),
                    _ => {
                        self.output
                            .log_warning(service, "unknown docker service (skipped)");
                        continue;
                    }
                };

                match bridge.start_service(service, docker_config) {
                    Ok(_) => {
                        if let Some(info) = bridge.get_connection(service) {
                            apply_service_env(&self.config, service, &info);
                        }
                        self.output
                            .log_system(&format!("docker service '{}' started", service));
                    }
                    Err(e) => {
                        self.output
                            .log_warning(service, &format!("docker start failed: {}", e));
                    }
                }
            }
        }

        if let Some(ref mut server) = self.server {
            if let Err(e) = server.start().await {
                self.output
                    .log_warning("devserver", &format!("failed to start: {}", e));
            } else {
                self.output
                    .log_system(&format!("Dev server running at {}", server.url()));
            }
        }

        let load_start = std::time::Instant::now();
        let mut loaded_count = 0;

        // Pre-load installed dependencies so they're available as import providers.
        let deps_dir = self.project_dir.join(".run").join("components");
        if deps_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&deps_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map(|e| e == "wasm").unwrap_or(false) {
                        let dep_name = path
                            .file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_default();
                        // Strip @version suffix: "foo@1.0.0" -> "foo"
                        let base_name = dep_name
                            .rsplit_once('@')
                            .map(|(name, _)| name.to_string())
                            .unwrap_or_else(|| dep_name.clone());
                        let bytes = match std::fs::read(&path) {
                            Ok(b) => b,
                            Err(_) => continue,
                        };
                        let mut runtime_lock = self.runtime.lock().unwrap();
                        match runtime_lock.load_component_bytes(&base_name, bytes) {
                            Ok(id) => {
                                drop(runtime_lock);
                                let _ = self.orchestrator.register(&id, vec![]);
                                self.output.log_system(&format!(
                                    "dependency '{}' loaded",
                                    base_name
                                ));
                            }
                            Err(_) => {}
                        }
                    }
                }
            }
        }

        for (name, comp_config) in &self.config.components {
            let wasm_path =
                resolve_component_path(&self.config, &self.project_dir, name, comp_config);
            if let Some(wasm_path) = wasm_path {
                if !wasm_path.exists() {
                    self.output
                        .log_error(name, &format!("file not found: {}", wasm_path.display()));
                    continue;
                }

                let comp_start = std::time::Instant::now();

                let component_id = {
                    let mut runtime_lock = self.runtime.lock().unwrap();
                    let bytes = match std::fs::read(&wasm_path) {
                        Ok(b) => b,
                        Err(e) => {
                            self.output.log_error(name, &format!("read failed: {}", e));
                            continue;
                        }
                    };
                    match runtime_lock.load_component_bytes(name, bytes) {
                        Ok(id) => id,
                        Err(e) => {
                            self.output.log_error(name, &format!("{}", e));
                            continue;
                        }
                    }
                };

                if let Err(e) = self
                    .orchestrator
                    .register(&component_id, comp_config.dependencies.clone())
                {
                    self.output.log_error(name, &format!("{}", e));
                    continue;
                }

                let mut caps = CapabilitySet::dev_default();
                for cap_str in &comp_config.capabilities {
                    if let Some(cap) = parse_capability_string(cap_str) {
                        caps.grant(cap);
                    }
                }

                match self.orchestrator.start(&component_id, caps) {
                    Ok(_) => {
                        let elapsed = comp_start.elapsed();
                        self.output
                            .log_component(name, &format!("started ({:.1?})", elapsed));
                        loaded_count += 1;
                    }
                    Err(e) => {
                        self.output.log_error(name, &format!("{}", e));
                    }
                }
            } else if comp_config.source.is_some() {
                self.output
                    .log_warning(name, "no output path resolved (run: run build)");
            } else {
                self.output.log_error(name, "no path or source specified");
            }
        }

        let total_elapsed = load_start.elapsed();
        self.output.log_system(&format!(
            "{} components ready ({:.1?})",
            loaded_count, total_elapsed
        ));

        let output_clone = self.output.clone();
        self.orchestrator
            .on_event(Box::new(move |event| match event {
                OrchestratorEvent::ComponentStopped { id, exit_code } => {
                    output_clone.log_component(id, &format!("stopped (exit {})", exit_code));
                }
                OrchestratorEvent::ComponentFailed { id, error } => {
                    output_clone.log_error(id, error);
                }
                OrchestratorEvent::ComponentRestarted { id, attempt } => {
                    output_clone.log_component(id, &format!("restarted (attempt {})", attempt));
                }
                OrchestratorEvent::ComponentCall { from, to, function } => {
                    if output_clone.is_verbose() {
                        output_clone.log_call(from, to, function);
                    }
                }
                _ => {}
            }));

        if let Some(ref mut watcher) = self.watcher {
            let orchestrator = Arc::clone(&self.orchestrator);
            let output = self.output.clone();
            let running = Arc::clone(&self.running);
            let config = self.config.clone();
            let project_dir = self.project_dir.clone();
            let server_notifier = self.server_notifier.clone();

            watcher.start(move |event| {
                if !running.load(std::sync::atomic::Ordering::SeqCst) {
                    return;
                }

                match event {
                    WatchEvent::Modified(ref path) | WatchEvent::Created(ref path) => {
                        if let Some(component_name) = find_component_for_file(path, &config) {
                            let build_start = std::time::Instant::now();

                            if let Err(e) = crate::v2::build::build_all(&config, &project_dir) {
                                output.log_error(&component_name, &format!("build failed: {}", e));
                                return;
                            }

                            let build_time = build_start.elapsed();
                            output.log_component(
                                &component_name,
                                &format!("rebuilt ({:.0?})", build_time),
                            );

                            let caps = CapabilitySet::dev_default();
                            if let Err(e) = orchestrator.restart(&component_name, caps) {
                                output.log_error(&component_name, &e.to_string());
                            } else {
                                output.log_component(&component_name, "reloaded");
                                if let Some(ref notifier) = server_notifier {
                                    notifier.notify_reload(&component_name);
                                }
                            }
                        } else {
                            let filename = path
                                .file_name()
                                .map(|f| f.to_string_lossy().to_string())
                                .unwrap_or_else(|| path.display().to_string());
                            output.log_warning(&filename, "changed (no component matched)");
                        }
                    }
                    WatchEvent::Deleted(path) => {
                        let filename = path
                            .file_name()
                            .map(|f| f.to_string_lossy().to_string())
                            .unwrap_or_else(|| path.display().to_string());
                        output.log_warning(&filename, "deleted (manual reload may be needed)");
                    }
                }
            })?;

            self.output
                .log_system("Hot reload enabled - watching for changes");
        }

        self.output
            .log_system(&format!("Listening on port {}", self.config.dev.port));
        self.output.print_ready();

        Ok(())
    }
    pub fn stop(&mut self) -> Result<()> {
        self.running
            .store(false, std::sync::atomic::Ordering::SeqCst);

        if let Some(ref mut watcher) = self.watcher {
            watcher.stop();
        }

        if let Some(ref mut bridge) = self.bridge {
            let _ = bridge.stop_all();
        }

        self.orchestrator.stop_all()?;

        if let Some(ref mut server) = self.server {
            server.stop();
        }

        self.output.log_system("Development session stopped");

        Ok(())
    }
    pub async fn wait(&self) {
        while self.running.load(std::sync::atomic::Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
    pub fn stats(&self) -> DevStats {
        let orch_metrics = self.orchestrator.orchestrator_metrics();

        DevStats {
            uptime_ms: self.start_time.elapsed().as_millis() as u64,
            components_running: orch_metrics.components_running,
            total_calls: orch_metrics.total_calls,
            total_errors: orch_metrics.total_errors,
            hot_reloads: 0,
        }
    }
}
#[derive(Debug, Clone)]
pub struct DevStats {
    pub uptime_ms: u64,
    pub components_running: usize,
    pub total_calls: u64,
    pub total_errors: u64,
    pub hot_reloads: u64,
}
fn find_component_for_file(file_path: &Path, config: &RunConfig) -> Option<String> {
    let file_path_str = file_path.to_string_lossy();

    for (name, comp_config) in &config.components {
        if let Some(ref comp_path) = comp_config.path {
            if file_path_str.contains(comp_path) {
                return Some(name.clone());
            }
        }

        if file_path_str.contains(name) {
            return Some(name.clone());
        }
    }

    let parts: Vec<&str> = file_path_str.split(std::path::MAIN_SEPARATOR).collect();
    for (i, part) in parts.iter().enumerate() {
        if *part == "src" || *part == "components" {
            if let Some(next) = parts.get(i + 1) {
                for name in config.components.keys() {
                    if name.contains(next) || next.contains(name.as_str()) {
                        return Some(name.clone());
                    }
                }
            }
        }
    }

    None
}

fn resolve_component_path(
    config: &RunConfig,
    project_dir: &Path,
    name: &str,
    comp_config: &crate::v2::config::ComponentConfig,
) -> Option<PathBuf> {
    if let Some(ref path) = comp_config.path {
        return Some(project_dir.join(path));
    }

    if let Some(ref source) = comp_config.source {
        let source_path = project_dir.join(source);
        if source_path
            .extension()
            .map(|e| e == "wasm")
            .unwrap_or(false)
        {
            return Some(source_path);
        }
    }

    let output_dir = project_dir.join(&config.build.output_dir);
    let candidate = output_dir.join(format!("{}.wasm", name));
    if candidate.exists() {
        return Some(candidate);
    }

    None
}
fn parse_capability_string(s: &str) -> Option<Capability> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.is_empty() {
        return None;
    }

    match parts[0] {
        "net" if parts.len() >= 3 => match parts[1] {
            "listen" => {
                let port = parts[2].parse().ok()?;
                Some(Capability::NetListen { port })
            }
            "connect" => {
                let host = parts.get(2).unwrap_or(&"*").to_string();
                let port = parts.get(3).and_then(|p| p.parse().ok()).unwrap_or(0);
                Some(Capability::NetConnect { host, port })
            }
            _ => None,
        },
        "fs" if parts.len() >= 3 => {
            let path = PathBuf::from(parts[2..].join(":"));
            match parts[1] {
                "read" => Some(Capability::FileRead(path)),
                "write" => Some(Capability::FileWrite(path)),
                _ => None,
            }
        }
        "env" => {
            if parts.len() > 1 {
                Some(Capability::EnvRead(parts[1].to_string()))
            } else {
                Some(Capability::EnvReadAll)
            }
        }
        "clock" => Some(Capability::Clock),
        "random" => Some(Capability::Random),
        "stdin" => Some(Capability::Stdin),
        "stdout" => Some(Capability::Stdout),
        "stderr" => Some(Capability::Stderr),
        "all" => Some(Capability::Unrestricted),
        _ => None,
    }
}

fn apply_service_env(config: &RunConfig, service: &str, info: &ConnectionInfo) {
    if let Some(service_config) = config.docker.services.get(service) {
        if let Some(env_var) = &service_config.env_var {
            let url = if !service_config.url.is_empty() {
                service_config.url.clone()
            } else {
                derive_service_url(service, info)
            };
            // SAFETY: This is single-threaded during dev session initialization
            unsafe { std::env::set_var(env_var, url) };
        }
    }
}

fn derive_service_url(service: &str, info: &ConnectionInfo) -> String {
    let service_lower = service.to_lowercase();
    match service_lower.as_str() {
        "postgres" | "postgresql" => {
            let port = info.ports.get(&5432).cloned().unwrap_or(5432);
            format!("postgres://postgres:secret@{}:{}/postgres", info.host, port)
        }
        "redis" => {
            let port = info.ports.get(&6379).cloned().unwrap_or(6379);
            format!("redis://{}:{}/0", info.host, port)
        }
        "mysql" => {
            let port = info.ports.get(&3306).cloned().unwrap_or(3306);
            format!("mysql://root:root@{}:{}/mysql", info.host, port)
        }
        _ => {
            format!(
                "{}://{}:{}",
                service_lower,
                info.host,
                info.ports.values().next().cloned().unwrap_or(0)
            )
        }
    }
}
pub async fn run_dev(options: DevOptions) -> Result<()> {
    let mut session = DevSession::new(options)?;
    session.start().await?;
    session.wait().await;
    session.stop()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_capability_string() {
        let cap = parse_capability_string("net:listen:8080").unwrap();
        assert!(matches!(cap, Capability::NetListen { port: 8080 }));

        let cap = parse_capability_string("fs:read:/data").unwrap();
        assert!(matches!(cap, Capability::FileRead(_)));

        let cap = parse_capability_string("env:API_KEY").unwrap();
        assert!(matches!(cap, Capability::EnvRead(_)));
    }
}
