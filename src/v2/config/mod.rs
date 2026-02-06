//! Configuration Schema

mod parser;
mod schema;

pub use parser::ConfigParser;
pub use schema::*;

use crate::v2::Result;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct RunConfig {
    pub project: ProjectConfig,
    pub dependencies: HashMap<String, DependencyConfig>,
    pub dev_dependencies: HashMap<String, DependencyConfig>,
    pub components: HashMap<String, ComponentConfig>,
    pub tests: HashMap<String, TestCaseConfig>,
    pub plugins: HashMap<String, PluginConfig>,
    pub deploy: HashMap<String, DeployConfig>,
    pub docker: DockerConfig,
    pub dev: DevConfig,
    pub build: BuildConfig,
    pub registry: RegistrySettings,
}

#[derive(Debug, Clone, Default)]
pub struct DockerConfig {
    pub services: HashMap<String, DockerService>,
}

#[derive(Debug, Clone)]
pub struct DockerService {
    pub url: String,
    pub env_var: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProjectConfig {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub authors: Vec<String>,
    pub license: Option<String>,
    pub repository: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DependencyConfig {
    pub version: String,
    pub optional: bool,
    pub features: Vec<String>,
    pub git: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ComponentConfig {
    pub path: Option<String>,
    pub source: Option<String>,
    pub language: Option<String>,
    pub build: Option<String>,
    pub capabilities: Vec<String>,
    pub env: HashMap<String, String>,
    pub dependencies: Vec<String>,
    pub health_check: Option<HealthCheckConfig>,
    pub restart: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HealthCheckConfig {
    pub function: String,
    pub interval: u64,
    pub timeout: u64,
    pub retries: u32,
}

#[derive(Debug, Clone)]
pub struct DevConfig {
    pub hot_reload: bool,
    pub watch: Vec<String>,
    pub port: u16,
    pub verbose: bool,
    pub services: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct BuildConfig {
    pub output_dir: String,
    pub target: String,
    pub opt_level: String,
    pub debug: bool,
    pub reproducible: bool,
    pub source_date_epoch: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct RegistrySettings {
    pub url: String,
    pub mirrors: Vec<String>,
    pub auth_token: Option<String>,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            project: ProjectConfig {
                name: "unnamed".to_string(),
                version: "0.1.0".to_string(),
                description: None,
                authors: vec![],
                license: None,
                repository: None,
            },
            dependencies: HashMap::new(),
            dev_dependencies: HashMap::new(),
            components: HashMap::new(),
            tests: HashMap::new(),
            plugins: HashMap::new(),
            deploy: HashMap::new(),
            docker: DockerConfig::default(),
            dev: DevConfig::default(),
            build: BuildConfig::default(),
            registry: RegistrySettings::default(),
        }
    }
}

impl Default for DevConfig {
    fn default() -> Self {
        Self {
            hot_reload: true,
            watch: vec!["src/**/*".to_string(), "wit/**/*.wit".to_string()],
            port: 3000,
            verbose: false,
            services: vec![],
        }
    }
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            output_dir: "target/wasm".to_string(),
            target: "wasm32-wasip2".to_string(),
            opt_level: "release".to_string(),
            debug: false,
            reproducible: false,
            source_date_epoch: None,
        }
    }
}

impl Default for RegistrySettings {
    fn default() -> Self {
        Self {
            url: "https://registry.esubalew.dev".to_string(),
            mirrors: vec![],
            auth_token: None,
        }
    }
}

impl RunConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        ConfigParser::parse(&content)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let content = self.serialize();
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn serialize(&self) -> String {
        let mut output = String::new();

        output.push_str("[project]\n");
        output.push_str(&format!("name = \"{}\"\n", self.project.name));
        output.push_str(&format!("version = \"{}\"\n", self.project.version));
        if let Some(ref desc) = self.project.description {
            output.push_str(&format!("description = \"{}\"\n", desc));
        }
        if !self.project.authors.is_empty() {
            let authors: Vec<String> = self
                .project
                .authors
                .iter()
                .map(|a| format!("\"{}\"", a))
                .collect();
            output.push_str(&format!("authors = [{}]\n", authors.join(", ")));
        }
        if let Some(ref license) = self.project.license {
            output.push_str(&format!("license = \"{}\"\n", license));
        }
        if let Some(ref repo) = self.project.repository {
            output.push_str(&format!("repository = \"{}\"\n", repo));
        }
        output.push('\n');

        if !self.dependencies.is_empty() {
            let mut simple = Vec::new();
            let mut complex = Vec::new();
            for (name, dep) in &self.dependencies {
                if dep.optional
                    || !dep.features.is_empty()
                    || dep.git.is_some()
                    || dep.path.is_some()
                {
                    complex.push((name, dep));
                } else {
                    simple.push((name, dep));
                }
            }
            if !simple.is_empty() {
                output.push_str("[dependencies]\n");
                for (name, dep) in simple {
                    output.push_str(&format!("\"{}\" = \"{}\"\n", name, dep.version));
                }
                output.push('\n');
            }
            for (name, dep) in complex {
                output.push_str(&format!("[dependencies.{}]\n", name));
                output.push_str(&format!("version = \"{}\"\n", dep.version));
                if dep.optional {
                    output.push_str("optional = true\n");
                }
                if let Some(ref git) = dep.git {
                    output.push_str(&format!("git = \"{}\"\n", git));
                }
                if let Some(ref path) = dep.path {
                    output.push_str(&format!("path = \"{}\"\n", path));
                }
                if !dep.features.is_empty() {
                    let feats: Vec<String> =
                        dep.features.iter().map(|f| format!("\"{}\"", f)).collect();
                    output.push_str(&format!("features = [{}]\n", feats.join(", ")));
                }
                output.push('\n');
            }
        }

        if !self.dev_dependencies.is_empty() {
            let mut simple = Vec::new();
            let mut complex = Vec::new();
            for (name, dep) in &self.dev_dependencies {
                if dep.optional
                    || !dep.features.is_empty()
                    || dep.git.is_some()
                    || dep.path.is_some()
                {
                    complex.push((name, dep));
                } else {
                    simple.push((name, dep));
                }
            }
            if !simple.is_empty() {
                output.push_str("[dev-dependencies]\n");
                for (name, dep) in simple {
                    output.push_str(&format!("\"{}\" = \"{}\"\n", name, dep.version));
                }
                output.push('\n');
            }
            for (name, dep) in complex {
                output.push_str(&format!("[dev-dependencies.{}]\n", name));
                output.push_str(&format!("version = \"{}\"\n", dep.version));
                if dep.optional {
                    output.push_str("optional = true\n");
                }
                if let Some(ref git) = dep.git {
                    output.push_str(&format!("git = \"{}\"\n", git));
                }
                if let Some(ref path) = dep.path {
                    output.push_str(&format!("path = \"{}\"\n", path));
                }
                if !dep.features.is_empty() {
                    let feats: Vec<String> =
                        dep.features.iter().map(|f| format!("\"{}\"", f)).collect();
                    output.push_str(&format!("features = [{}]\n", feats.join(", ")));
                }
                output.push('\n');
            }
        }

        for (name, comp) in &self.components {
            output.push_str(&format!("[components.{}]\n", name));
            if let Some(ref path) = comp.path {
                output.push_str(&format!("path = \"{}\"\n", path));
            }
            if let Some(ref source) = comp.source {
                output.push_str(&format!("source = \"{}\"\n", source));
            }
            if let Some(ref lang) = comp.language {
                output.push_str(&format!("language = \"{}\"\n", lang));
            }
            if let Some(ref build) = comp.build {
                output.push_str(&format!("build = \"{}\"\n", build));
            }
            if !comp.capabilities.is_empty() {
                let caps: Vec<String> = comp
                    .capabilities
                    .iter()
                    .map(|c| format!("\"{}\"", c))
                    .collect();
                output.push_str(&format!("capabilities = [{}]\n", caps.join(", ")));
            }
            if !comp.dependencies.is_empty() {
                let deps: Vec<String> = comp
                    .dependencies
                    .iter()
                    .map(|d| format!("\"{}\"", d))
                    .collect();
                output.push_str(&format!("dependencies = [{}]\n", deps.join(", ")));
            }
            if let Some(ref restart) = comp.restart {
                output.push_str(&format!("restart = \"{}\"\n", restart));
            }
            output.push('\n');

            if !comp.env.is_empty() {
                output.push_str(&format!("[components.{}.env]\n", name));
                for (key, value) in &comp.env {
                    output.push_str(&format!("{} = \"{}\"\n", key, value));
                }
                output.push('\n');
            }

            if let Some(ref health) = comp.health_check {
                output.push_str(&format!("[components.{}.health_check]\n", name));
                output.push_str(&format!("function = \"{}\"\n", health.function));
                output.push_str(&format!("interval = {}\n", health.interval));
                output.push_str(&format!("timeout = {}\n", health.timeout));
                output.push_str(&format!("retries = {}\n", health.retries));
                output.push('\n');
            }
        }

        if !self.tests.is_empty() {
            for (name, test) in &self.tests {
                output.push_str(&format!("[tests.{}]\n", name));
                output.push_str(&format!("component = \"{}\"\n", test.component));
                output.push_str(&format!("function = \"{}\"\n", test.function));
                if !test.args.is_empty() {
                    let args: Vec<String> =
                        test.args.iter().map(|a| format!("\"{}\"", a)).collect();
                    output.push_str(&format!("args = [{}]\n", args.join(", ")));
                }
                if let Some(ref expect) = test.expect {
                    output.push_str(&format!("expect = \"{}\"\n", expect));
                }
                if let Some(exit) = test.expect_exit {
                    output.push_str(&format!("expect_exit = {}\n", exit));
                }
                if let Some(ref err) = test.expect_error {
                    output.push_str(&format!("expect_error = \"{}\"\n", err));
                }
                output.push('\n');
            }
        }

        if !self.plugins.is_empty() {
            for (name, plugin) in &self.plugins {
                output.push_str(&format!("[plugins.{}]\n", name));
                if let Some(ref path) = plugin.path {
                    output.push_str(&format!("path = \"{}\"\n", path));
                }
                if let Some(ref package) = plugin.package {
                    output.push_str(&format!("package = \"{}\"\n", package));
                }
                if let Some(ref version) = plugin.version {
                    output.push_str(&format!("version = \"{}\"\n", version));
                }
                output.push_str(&format!("enabled = {}\n", plugin.enabled));
                if !plugin.hooks.is_empty() {
                    let hooks: Vec<String> =
                        plugin.hooks.iter().map(|h| format!("\"{}\"", h)).collect();
                    output.push_str(&format!("hooks = [{}]\n", hooks.join(", ")));
                }
                output.push('\n');
            }
        }

        if !self.deploy.is_empty() {
            for (name, deploy) in &self.deploy {
                output.push_str(&format!("[deploy.{}]\n", name));
                output.push_str(&format!("target = \"{}\"\n", deploy.target_type));
                for (key, value) in &deploy.options {
                    output.push_str(&format!("{} = \"{}\"\n", key, value));
                }
                output.push('\n');
            }
        }

        output.push_str("[dev]\n");
        output.push_str(&format!("hot_reload = {}\n", self.dev.hot_reload));
        output.push_str(&format!("port = {}\n", self.dev.port));
        output.push_str(&format!("verbose = {}\n", self.dev.verbose));
        if !self.dev.watch.is_empty() {
            let watch: Vec<String> = self
                .dev
                .watch
                .iter()
                .map(|w| format!("\"{}\"", w))
                .collect();
            output.push_str(&format!("watch = [{}]\n", watch.join(", ")));
        }
        if !self.dev.services.is_empty() {
            let services: Vec<String> = self
                .dev
                .services
                .iter()
                .map(|s| format!("\"{}\"", s))
                .collect();
            output.push_str(&format!("services = [{}]\n", services.join(", ")));
        }
        output.push('\n');

        output.push_str("[build]\n");
        output.push_str(&format!("output_dir = \"{}\"\n", self.build.output_dir));
        output.push_str(&format!("target = \"{}\"\n", self.build.target));
        output.push_str(&format!("opt_level = \"{}\"\n", self.build.opt_level));
        output.push_str(&format!("debug = {}\n", self.build.debug));
        output.push_str(&format!("reproducible = {}\n", self.build.reproducible));
        if let Some(epoch) = self.build.source_date_epoch {
            output.push_str(&format!("source_date_epoch = {}\n", epoch));
        }
        output.push('\n');

        output.push_str("[registry]\n");
        output.push_str(&format!("url = \"{}\"\n", self.registry.url));
        if !self.registry.mirrors.is_empty() {
            let mirrors: Vec<String> = self
                .registry
                .mirrors
                .iter()
                .map(|m| format!("\"{}\"", m))
                .collect();
            output.push_str(&format!("mirrors = [{}]\n", mirrors.join(", ")));
        }
        if let Some(ref token) = self.registry.auth_token {
            output.push_str(&format!("auth_token = \"{}\"\n", token));
        }
        output.push('\n');

        if !self.docker.services.is_empty() {
            for (name, service) in &self.docker.services {
                output.push_str(&format!("[docker.{}]\n", name));
                output.push_str(&format!("url = \"{}\"\n", service.url));
                if let Some(ref env_var) = service.env_var {
                    output.push_str(&format!("env_var = \"{}\"\n", env_var));
                }
                output.push('\n');
            }
        }

        output
    }
}
