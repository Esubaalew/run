//! TOML Parser

use super::*;
use crate::v2::Result;
use std::collections::HashMap;

pub struct ConfigParser;

impl ConfigParser {
    pub fn parse(content: &str) -> Result<RunConfig> {
        let mut config = RunConfig::default();
        let mut current_section = Section::None;
        let mut current_subsection: Option<String> = None;

        for line in content.lines() {
            let line = line.trim();

            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if line.starts_with('[') && line.ends_with(']') {
                let section_name = &line[1..line.len() - 1];
                (current_section, current_subsection) = parse_section_header(section_name);
                continue;
            }

            if let Some((key, value)) = parse_key_value(line) {
                match current_section {
                    Section::Project => {
                        parse_project_field(&mut config.project, &key, &value)?;
                    }
                    Section::Dependencies => {
                        if let Some(ref subsection) = current_subsection {
                            let dep = config
                                .dependencies
                                .entry(subsection.clone())
                                .or_insert_with(|| DependencyConfig {
                                    version: String::new(),
                                    optional: false,
                                    features: vec![],
                                    git: None,
                                    path: None,
                                });
                            parse_dependency_field(dep, &key, &value)?;
                        } else {
                            let name = key.trim_matches('"');
                            let version = value.trim_matches('"');
                            config.dependencies.insert(
                                name.to_string(),
                                DependencyConfig {
                                    version: version.to_string(),
                                    optional: false,
                                    features: vec![],
                                    git: None,
                                    path: None,
                                },
                            );
                        }
                    }
                    Section::DevDependencies => {
                        if let Some(ref subsection) = current_subsection {
                            let dep = config
                                .dev_dependencies
                                .entry(subsection.clone())
                                .or_insert_with(|| DependencyConfig {
                                    version: String::new(),
                                    optional: false,
                                    features: vec![],
                                    git: None,
                                    path: None,
                                });
                            parse_dependency_field(dep, &key, &value)?;
                        } else {
                            let name = key.trim_matches('"');
                            let version = value.trim_matches('"');
                            config.dev_dependencies.insert(
                                name.to_string(),
                                DependencyConfig {
                                    version: version.to_string(),
                                    optional: false,
                                    features: vec![],
                                    git: None,
                                    path: None,
                                },
                            );
                        }
                    }
                    Section::Components => {
                        if let Some(ref subsection) = current_subsection {
                            let comp =
                                config
                                    .components
                                    .entry(subsection.clone())
                                    .or_insert_with(|| ComponentConfig {
                                        path: None,
                                        source: None,
                                        language: None,
                                        build: None,
                                        capabilities: vec![],
                                        env: HashMap::new(),
                                        dependencies: vec![],
                                        health_check: None,
                                        restart: None,
                                    });
                            parse_component_field(comp, &key, &value)?;
                        }
                    }
                    Section::ComponentEnv => {
                        if let Some(ref subsection) = current_subsection {
                            let comp =
                                config
                                    .components
                                    .entry(subsection.clone())
                                    .or_insert_with(|| ComponentConfig {
                                        path: None,
                                        source: None,
                                        language: None,
                                        build: None,
                                        capabilities: vec![],
                                        env: HashMap::new(),
                                        dependencies: vec![],
                                        health_check: None,
                                        restart: None,
                                    });
                            comp.env.insert(key, value.trim_matches('"').to_string());
                        }
                    }
                    Section::ComponentHealthCheck => {
                        if let Some(ref subsection) = current_subsection {
                            let comp =
                                config
                                    .components
                                    .entry(subsection.clone())
                                    .or_insert_with(|| ComponentConfig {
                                        path: None,
                                        source: None,
                                        language: None,
                                        build: None,
                                        capabilities: vec![],
                                        env: HashMap::new(),
                                        dependencies: vec![],
                                        health_check: None,
                                        restart: None,
                                    });
                            if comp.health_check.is_none() {
                                comp.health_check = Some(HealthCheckConfig {
                                    function: "health".to_string(),
                                    interval: 30,
                                    timeout: 5,
                                    retries: 3,
                                });
                            }
                            if let Some(ref mut hc) = comp.health_check {
                                match key.as_str() {
                                    "function" => hc.function = value.trim_matches('"').to_string(),
                                    "interval" => {
                                        hc.interval = value.parse().unwrap_or(hc.interval)
                                    }
                                    "timeout" => hc.timeout = value.parse().unwrap_or(hc.timeout),
                                    "retries" => hc.retries = value.parse().unwrap_or(hc.retries),
                                    _ => {}
                                }
                            }
                        }
                    }
                    Section::Tests => {
                        if let Some(ref subsection) = current_subsection {
                            let test =
                                config.tests.entry(subsection.clone()).or_insert_with(|| {
                                    TestCaseConfig {
                                        component: String::new(),
                                        function: String::new(),
                                        args: vec![],
                                        expect: None,
                                        expect_exit: None,
                                        expect_error: None,
                                    }
                                });
                            parse_test_field(test, &key, &value)?;
                        }
                    }
                    Section::Plugins => {
                        if let Some(ref subsection) = current_subsection {
                            let plugin =
                                config.plugins.entry(subsection.clone()).or_insert_with(|| {
                                    PluginConfig {
                                        path: None,
                                        package: None,
                                        version: None,
                                        enabled: true,
                                        hooks: vec![],
                                    }
                                });
                            parse_plugin_field(plugin, &key, &value)?;
                        }
                    }
                    Section::Deploy => {
                        if let Some(ref subsection) = current_subsection {
                            let deploy =
                                config.deploy.entry(subsection.clone()).or_insert_with(|| {
                                    DeployConfig {
                                        name: subsection.clone(),
                                        target_type: String::new(),
                                        options: HashMap::new(),
                                    }
                                });
                            parse_deploy_field(deploy, &key, &value)?;
                        }
                    }
                    Section::Docker => {
                        if let Some(ref subsection) = current_subsection {
                            let service = config
                                .docker
                                .services
                                .entry(subsection.clone())
                                .or_insert_with(|| crate::v2::config::DockerService {
                                    url: String::new(),
                                    env_var: None,
                                });
                            parse_docker_service_field(service, &key, &value)?;
                        }
                    }
                    Section::Dev => {
                        parse_dev_field(&mut config.dev, &key, &value)?;
                    }
                    Section::Build => {
                        parse_build_field(&mut config.build, &key, &value)?;
                    }
                    Section::Registry => {
                        parse_registry_field(&mut config.registry, &key, &value)?;
                    }
                    Section::None => {}
                }
            }
        }

        Ok(config)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Section {
    None,
    Project,
    Dependencies,
    DevDependencies,
    Components,
    ComponentEnv,
    ComponentHealthCheck,
    Tests,
    Plugins,
    Deploy,
    Docker,
    Dev,
    Build,
    Registry,
}

fn parse_section_header(header: &str) -> (Section, Option<String>) {
    let parts: Vec<&str> = header.split('.').collect();
    let strip = |s: &str| s.trim_matches('"').to_string();

    match parts.as_slice() {
        ["project"] => (Section::Project, None),
        ["dependencies"] => (Section::Dependencies, None),
        ["dependencies", name] => (Section::Dependencies, Some(strip(name))),
        ["dev-dependencies"] | ["dev_dependencies"] => (Section::DevDependencies, None),
        ["dev-dependencies", name] | ["dev_dependencies", name] => {
            (Section::DevDependencies, Some(strip(name)))
        }
        ["components", name] => (Section::Components, Some(strip(name))),
        ["components", name, "env"] => (Section::ComponentEnv, Some(strip(name))),
        ["components", name, "health_check"] => (Section::ComponentHealthCheck, Some(strip(name))),
        ["tests"] => (Section::Tests, None),
        ["tests", name] => (Section::Tests, Some(strip(name))),
        ["plugins"] => (Section::Plugins, None),
        ["plugins", name] => (Section::Plugins, Some(strip(name))),
        ["deploy"] => (Section::Deploy, None),
        ["deploy", name] => (Section::Deploy, Some(strip(name))),
        ["docker"] => (Section::Docker, None),
        ["docker", name] => (Section::Docker, Some(strip(name))),
        ["dev"] => (Section::Dev, None),
        ["build"] => (Section::Build, None),
        ["registry"] => (Section::Registry, None),
        _ => (Section::None, None),
    }
}

fn parse_key_value(line: &str) -> Option<(String, String)> {
    let eq_pos = line.find('=')?;
    let key = line[..eq_pos].trim().to_string();
    let value = line[eq_pos + 1..].trim().to_string();
    Some((key, value))
}

fn parse_project_field(project: &mut ProjectConfig, key: &str, value: &str) -> Result<()> {
    match key {
        "name" => project.name = value.trim_matches('"').to_string(),
        "version" => project.version = value.trim_matches('"').to_string(),
        "description" => project.description = Some(value.trim_matches('"').to_string()),
        "license" => project.license = Some(value.trim_matches('"').to_string()),
        "repository" => project.repository = Some(value.trim_matches('"').to_string()),
        "authors" => project.authors = parse_string_array(value),
        _ => {}
    }
    Ok(())
}

fn parse_dependency_field(dep: &mut DependencyConfig, key: &str, value: &str) -> Result<()> {
    match key {
        "version" => dep.version = value.trim_matches('"').to_string(),
        "optional" => dep.optional = value == "true",
        "git" => dep.git = Some(value.trim_matches('"').to_string()),
        "path" => dep.path = Some(value.trim_matches('"').to_string()),
        "features" => dep.features = parse_string_array(value),
        _ => {}
    }
    Ok(())
}

fn parse_component_field(comp: &mut ComponentConfig, key: &str, value: &str) -> Result<()> {
    match key {
        "path" => comp.path = Some(value.trim_matches('"').to_string()),
        "source" => comp.source = Some(value.trim_matches('"').to_string()),
        "language" => comp.language = Some(value.trim_matches('"').to_string()),
        "build" => comp.build = Some(value.trim_matches('"').to_string()),
        "capabilities" => comp.capabilities = parse_string_array(value),
        "dependencies" => comp.dependencies = parse_string_array(value),
        "restart" => comp.restart = Some(value.trim_matches('"').to_string()),
        _ => {}
    }
    Ok(())
}

fn parse_test_field(test: &mut TestCaseConfig, key: &str, value: &str) -> Result<()> {
    match key {
        "component" => test.component = value.trim_matches('"').to_string(),
        "function" => test.function = value.trim_matches('"').to_string(),
        "args" => test.args = parse_string_array(value),
        "expect" => test.expect = Some(value.trim_matches('"').to_string()),
        "expect_exit" => test.expect_exit = value.parse().ok(),
        "expect_error" => test.expect_error = Some(value.trim_matches('"').to_string()),
        _ => {}
    }
    Ok(())
}

fn parse_plugin_field(plugin: &mut PluginConfig, key: &str, value: &str) -> Result<()> {
    match key {
        "path" => plugin.path = Some(value.trim_matches('"').to_string()),
        "package" => plugin.package = Some(value.trim_matches('"').to_string()),
        "version" => plugin.version = Some(value.trim_matches('"').to_string()),
        "enabled" => plugin.enabled = value == "true",
        "hooks" => plugin.hooks = parse_string_array(value),
        _ => {}
    }
    Ok(())
}

fn parse_deploy_field(deploy: &mut DeployConfig, key: &str, value: &str) -> Result<()> {
    match key {
        "name" => deploy.name = value.trim_matches('"').to_string(),
        "target" | "target_type" => deploy.target_type = value.trim_matches('"').to_string(),
        _ => {
            deploy
                .options
                .insert(key.to_string(), value.trim_matches('"').to_string());
        }
    }
    Ok(())
}

fn parse_dev_field(dev: &mut DevConfig, key: &str, value: &str) -> Result<()> {
    match key {
        "hot_reload" => dev.hot_reload = value == "true",
        "port" => dev.port = value.parse().unwrap_or(3000),
        "verbose" => dev.verbose = value == "true",
        "watch" => dev.watch = parse_string_array(value),
        "services" => dev.services = parse_string_array(value),
        _ => {}
    }
    Ok(())
}

fn parse_build_field(build: &mut BuildConfig, key: &str, value: &str) -> Result<()> {
    match key {
        "output_dir" => build.output_dir = value.trim_matches('"').to_string(),
        "target" => build.target = value.trim_matches('"').to_string(),
        "opt_level" => build.opt_level = value.trim_matches('"').to_string(),
        "debug" => build.debug = value == "true",
        "reproducible" => build.reproducible = value == "true",
        "source_date_epoch" => build.source_date_epoch = value.parse().ok(),
        _ => {}
    }
    Ok(())
}

fn parse_registry_field(registry: &mut RegistrySettings, key: &str, value: &str) -> Result<()> {
    match key {
        "url" => registry.url = value.trim_matches('"').to_string(),
        "mirrors" => registry.mirrors = parse_string_array(value),
        "auth_token" => registry.auth_token = Some(value.trim_matches('"').to_string()),
        _ => {}
    }
    Ok(())
}

fn parse_docker_service_field(
    service: &mut crate::v2::config::DockerService,
    key: &str,
    value: &str,
) -> Result<()> {
    match key {
        "url" => service.url = value.trim_matches('"').to_string(),
        "env_var" | "env" => service.env_var = Some(value.trim_matches('"').to_string()),
        _ => {}
    }
    Ok(())
}

fn parse_string_array(value: &str) -> Vec<String> {
    let value = value.trim();
    if !value.starts_with('[') || !value.ends_with(']') {
        return vec![];
    }

    let inner = &value[1..value.len() - 1];
    inner
        .split(',')
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_config() {
        let content = r#"
[project]
name = "my-app"
version = "1.0.0"

[dependencies]
"wasi:http" = "0.2.0"

[components.api]
path = "api.wasm"
capabilities = ["net:listen:8080"]

[dev]
hot_reload = true
port = 3000
"#;

        let config = ConfigParser::parse(content).unwrap();
        assert_eq!(config.project.name, "my-app");
        assert_eq!(config.project.version, "1.0.0");
        assert!(config.dependencies.contains_key("wasi:http"));
        assert!(config.components.contains_key("api"));
        assert!(config.dev.hot_reload);
        assert_eq!(config.dev.port, 3000);
    }

    #[test]
    fn test_parse_string_array() {
        let arr = parse_string_array(r#"["a", "b", "c"]"#);
        assert_eq!(arr, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_parse_section_header() {
        let (section, sub) = parse_section_header("project");
        assert_eq!(section, Section::Project);
        assert!(sub.is_none());

        let (section, sub) = parse_section_header("components.api");
        assert_eq!(section, Section::Components);
        assert_eq!(sub, Some("api".to_string()));
    }

    #[test]
    fn test_parse_tests_plugins_deploy() {
        let content = r#"
[tests.sample]
component = "api"
function = "health"
args = ["bool:true"]
expect = "bool:true"

[plugins.audit]
path = "plugins/audit.wasm"
enabled = true
hooks = ["on_build"]

[deploy.local]
target = "local"
output_dir = "dist/deploy"
"#;

        let config = ConfigParser::parse(content).unwrap();
        let test = config.tests.get("sample").unwrap();
        assert_eq!(test.component, "api");
        assert_eq!(test.function, "health");

        let plugin = config.plugins.get("audit").unwrap();
        assert_eq!(plugin.path.as_deref(), Some("plugins/audit.wasm"));
        assert!(plugin.enabled);

        let deploy = config.deploy.get("local").unwrap();
        assert_eq!(deploy.target_type, "local");
        assert_eq!(
            deploy.options.get("output_dir").map(|s| s.as_str()),
            Some("dist/deploy")
        );
    }
}
