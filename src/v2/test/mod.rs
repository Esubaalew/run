//! Test Runner
//!
//! `run test` executes deterministic component tests defined in run.toml.

use crate::v2::config::{RunConfig, TestCaseConfig};
use crate::v2::runtime::{CapabilitySet, ComponentValue, RuntimeConfig, RuntimeEngine};
use crate::v2::{Error, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct TestOptions {
    pub project_dir: PathBuf,
    pub component: Option<String>,
    pub build: bool,
    pub json: bool,
}

#[derive(Debug, Clone)]
pub struct TestReport {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
}

pub fn run_tests(options: TestOptions) -> Result<TestReport> {
    let config_path = options.project_dir.join("run.toml");
    let config = RunConfig::load(&config_path)?;

    if options.build {
        crate::v2::build::build_all(&config, &options.project_dir)?;
    }

    let mut engine = RuntimeEngine::new(RuntimeConfig::production())?;
    let mut handles: HashMap<String, crate::v2::runtime::InstanceHandle> = HashMap::new();

    let mut total = 0;
    let mut passed = 0;
    let mut failed = 0;

    for (name, test) in &config.tests {
        if let Some(ref filter) = options.component {
            if &test.component != filter && name != filter {
                continue;
            }
        }

        total += 1;

        match run_single_test(
            &config,
            &options.project_dir,
            &mut engine,
            &mut handles,
            test,
        ) {
            Ok(()) => {
                passed += 1;
                if !options.json {
                    println!("PASS {}", name);
                }
            }
            Err(e) => {
                failed += 1;
                if options.json {
                    println!(
                        "{{\"test\":\"{}\",\"status\":\"failed\",\"error\":\"{}\"}}",
                        name,
                        sanitize_json(&e.to_string())
                    );
                } else {
                    println!("FAIL {} - {}", name, e);
                }
            }
        }
    }

    if total == 0 && !options.json {
        println!("No tests defined. Add [tests.<name>] to run.toml.");
    }

    Ok(TestReport {
        total,
        passed,
        failed,
    })
}

fn run_single_test(
    config: &RunConfig,
    project_dir: &Path,
    engine: &mut RuntimeEngine,
    handles: &mut HashMap<String, crate::v2::runtime::InstanceHandle>,
    test: &TestCaseConfig,
) -> Result<()> {
    let wasm_path = resolve_component_path(config, project_dir, &test.component)?;

    let handle = if let Some(handle) = handles.get(&test.component) {
        handle.clone()
    } else {
        let component_id = engine.load_component(&wasm_path)?;
        let mut caps = CapabilitySet::deterministic();
        if let Some(comp) = config.components.get(&test.component) {
            for cap in &comp.capabilities {
                if let Some(parsed) = parse_capability_string(cap) {
                    caps.grant(parsed);
                }
            }
        }
        let handle = engine.instantiate(&component_id, caps)?;
        handles.insert(test.component.clone(), handle.clone());
        handle
    };

    let args = test
        .args
        .iter()
        .map(|a| ComponentValue::parse(a))
        .collect::<Result<Vec<_>>>()?;

    match engine.call(&handle, &test.function, args) {
        Ok(result) => {
            if let Some(exit) = test.expect_exit {
                if result.exit_code != exit {
                    return Err(Error::other(format!(
                        "Expected exit {}, got {}",
                        exit, result.exit_code
                    )));
                }
            }
            if let Some(ref expected) = test.expect {
                let expected_val = ComponentValue::parse(expected)?;
                let actual = result.return_value.unwrap_or(ComponentValue::Unit);
                if !component_value_eq(&expected_val, &actual) {
                    return Err(Error::other(format!(
                        "Expected {:?}, got {:?}",
                        expected_val, actual
                    )));
                }
            }
            Ok(())
        }
        Err(e) => {
            if let Some(ref expected) = test.expect_error {
                let message = e.to_string();
                if message.contains(expected) {
                    Ok(())
                } else {
                    Err(Error::other(format!(
                        "Expected error containing '{}', got '{}'",
                        expected, message
                    )))
                }
            } else {
                Err(e)
            }
        }
    }
}

fn resolve_component_path(
    config: &RunConfig,
    project_dir: &Path,
    component: &str,
) -> Result<PathBuf> {
    let comp_config = config
        .components
        .get(component)
        .ok_or_else(|| Error::ComponentNotFound(component.to_string()))?;

    if let Some(ref path) = comp_config.path {
        return Ok(project_dir.join(path));
    }

    if let Some(ref source) = comp_config.source {
        let source_path = project_dir.join(source);
        if source_path
            .extension()
            .map(|e| e == "wasm")
            .unwrap_or(false)
        {
            return Ok(source_path);
        }
    }

    let output_dir = project_dir.join(&config.build.output_dir);
    Ok(output_dir.join(format!("{}.wasm", component)))
}

fn component_value_eq(a: &ComponentValue, b: &ComponentValue) -> bool {
    match (a, b) {
        (ComponentValue::Bool(x), ComponentValue::Bool(y)) => x == y,
        (ComponentValue::U8(x), ComponentValue::U8(y)) => x == y,
        (ComponentValue::U16(x), ComponentValue::U16(y)) => x == y,
        (ComponentValue::U32(x), ComponentValue::U32(y)) => x == y,
        (ComponentValue::U64(x), ComponentValue::U64(y)) => x == y,
        (ComponentValue::S8(x), ComponentValue::S8(y)) => x == y,
        (ComponentValue::S16(x), ComponentValue::S16(y)) => x == y,
        (ComponentValue::S32(x), ComponentValue::S32(y)) => x == y,
        (ComponentValue::S64(x), ComponentValue::S64(y)) => x == y,
        (ComponentValue::F32(x), ComponentValue::F32(y)) => x == y,
        (ComponentValue::F64(x), ComponentValue::F64(y)) => x == y,
        (ComponentValue::Char(x), ComponentValue::Char(y)) => x == y,
        (ComponentValue::String(x), ComponentValue::String(y)) => x == y,
        (ComponentValue::Enum(x), ComponentValue::Enum(y)) => x == y,
        (ComponentValue::List(x), ComponentValue::List(y)) => {
            x.len() == y.len()
                && x.iter()
                    .zip(y.iter())
                    .all(|(a, b)| component_value_eq(a, b))
        }
        (ComponentValue::Record(x), ComponentValue::Record(y)) => {
            x.len() == y.len()
                && x.iter()
                    .zip(y.iter())
                    .all(|((kx, vx), (ky, vy))| kx == ky && component_value_eq(vx, vy))
        }
        (ComponentValue::Tuple(x), ComponentValue::Tuple(y)) => {
            x.len() == y.len()
                && x.iter()
                    .zip(y.iter())
                    .all(|(a, b)| component_value_eq(a, b))
        }
        (
            ComponentValue::Variant { tag: tx, value: vx },
            ComponentValue::Variant { tag: ty, value: vy },
        ) => {
            tx == ty
                && match (vx, vy) {
                    (None, None) => true,
                    (Some(a), Some(b)) => component_value_eq(a, b),
                    _ => false,
                }
        }
        (ComponentValue::Option(x), ComponentValue::Option(y)) => match (x, y) {
            (None, None) => true,
            (Some(a), Some(b)) => component_value_eq(a, b),
            _ => false,
        },
        (
            ComponentValue::Result { ok: okx, err: errx },
            ComponentValue::Result { ok: oky, err: erry },
        ) => match (okx, oky, errx, erry) {
            (Some(a), Some(b), None, None) => component_value_eq(a, b),
            (None, None, Some(a), Some(b)) => component_value_eq(a, b),
            (None, None, None, None) => true,
            _ => false,
        },
        (ComponentValue::Flags(x), ComponentValue::Flags(y)) => x == y,
        (ComponentValue::Handle(x), ComponentValue::Handle(y)) => x == y,
        (ComponentValue::Unit, ComponentValue::Unit) => true,
        _ => false,
    }
}

fn parse_capability_string(s: &str) -> Option<crate::v2::runtime::Capability> {
    use crate::v2::runtime::Capability;
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

fn sanitize_json(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code)]
    fn parse_component_value(s: &str) -> Result<ComponentValue> {
        if let Some((type_str, value_str)) = s.split_once(':') {
            match type_str {
                "s32" => value_str
                    .parse::<i32>()
                    .map(ComponentValue::S32)
                    .map_err(|e| Error::other(format!("Failed to parse s32: {}", e))),
                "s64" => value_str
                    .parse::<i64>()
                    .map(ComponentValue::S64)
                    .map_err(|e| Error::other(format!("Failed to parse s64: {}", e))),
                "u32" => value_str
                    .parse::<u32>()
                    .map(ComponentValue::U32)
                    .map_err(|e| Error::other(format!("Failed to parse u32: {}", e))),
                "u64" => value_str
                    .parse::<u64>()
                    .map(ComponentValue::U64)
                    .map_err(|e| Error::other(format!("Failed to parse u64: {}", e))),
                "f32" => value_str
                    .parse::<f32>()
                    .map(ComponentValue::F32)
                    .map_err(|e| Error::other(format!("Failed to parse f32: {}", e))),
                "f64" => value_str
                    .parse::<f64>()
                    .map(ComponentValue::F64)
                    .map_err(|e| Error::other(format!("Failed to parse f64: {}", e))),
                "bool" => Ok(ComponentValue::Bool(value_str == "true")),
                "string" => Ok(ComponentValue::String(value_str.to_string())),
                "enum" => Ok(ComponentValue::Enum(value_str.to_string())),
                _ => Err(Error::other(format!("Unknown type: {}", type_str))),
            }
        } else if s == "unit" {
            Ok(ComponentValue::Unit)
        } else {
            Err(Error::other(format!(
                "Invalid component value format: {}",
                s
            )))
        }
    }

    #[test]
    fn test_parse_component_value() {
        assert!(matches!(
            parse_component_value("s32:5").unwrap(),
            ComponentValue::S32(5)
        ));
        assert!(matches!(
            parse_component_value("bool:true").unwrap(),
            ComponentValue::Bool(true)
        ));
        assert!(matches!(
            parse_component_value("string:hello").unwrap(),
            ComponentValue::String(_)
        ));
        assert!(matches!(
            parse_component_value("enum:ready").unwrap(),
            ComponentValue::Enum(_)
        ));
        assert!(matches!(
            parse_component_value("unit").unwrap(),
            ComponentValue::Unit
        ));
    }
}
