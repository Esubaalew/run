mod async_exec;
mod capabilities;
mod engine;
mod instance;
mod linker;
mod memory;
#[cfg(feature = "v2")]
mod wasi_ctx;

pub use async_exec::{
    AsyncBatchExecutor, AsyncCallResult, AsyncConfig, AsyncEvent, AsyncMetrics, call_async,
    call_parallel,
};
pub use capabilities::{Capability, CapabilitySet, PolicyMode, SecurityPolicy};
pub use engine::{CliContext, LoadedComponentInfo, RuntimeConfig, RuntimeEngine};
pub use instance::{ComponentInstance, InstanceHandle, InstanceState};
pub use linker::{ComponentLinker, LinkageError};
pub use memory::{MemoryConfig, MemoryPool};
#[cfg(feature = "v2")]
pub use wasi_ctx::WasiCtxBuilder;

use crate::v2::{Error, Result};
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone, Default)]
pub struct RuntimeStats {
    pub components_loaded: usize,
    pub instantiations: u64,
    pub function_calls: u64,
    pub memory_bytes: usize,
    pub startup_ms: u64,
}

pub struct LoadedComponent {
    pub id: String,
    pub bytes: Arc<[u8]>,
    pub wit: Option<crate::v2::wit::WitPackage>,
    pub hash: String,
    pub source_path: Option<std::path::PathBuf>,
    #[cfg(feature = "v2")]
    pub compiled: Option<wasmtime::component::Component>,
}

impl std::fmt::Debug for LoadedComponent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadedComponent")
            .field("id", &self.id)
            .field("bytes_len", &self.bytes.len())
            .field("wit", &self.wit)
            .field("hash", &self.hash)
            .field("source_path", &self.source_path)
            .finish()
    }
}

impl LoadedComponent {
    pub fn from_file(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        let hash = compute_sha256(&bytes);

        Ok(Self {
            id: path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string(),
            bytes: bytes.into(),
            wit: None,
            hash,
            source_path: Some(path.to_path_buf()),
            #[cfg(feature = "v2")]
            compiled: None,
        })
    }

    pub fn from_bytes(id: &str, bytes: Vec<u8>) -> Self {
        let hash = compute_sha256(&bytes);
        Self {
            id: id.to_string(),
            bytes: bytes.into(),
            wit: None,
            hash,
            source_path: None,
            #[cfg(feature = "v2")]
            compiled: None,
        }
    }

    pub fn verify_hash(&self, expected: &str) -> bool {
        self.hash == expected
    }
}

fn compute_sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

#[derive(Debug)]
pub struct ExecutionResult {
    pub exit_code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub duration_ms: u64,
    pub return_value: Option<ComponentValue>,
}

#[derive(Debug, Clone)]
pub enum ComponentValue {
    Bool(bool),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    S8(i8),
    S16(i16),
    S32(i32),
    S64(i64),
    F32(f32),
    F64(f64),
    Char(char),
    String(String),
    List(Vec<ComponentValue>),
    Record(Vec<(String, ComponentValue)>),
    Tuple(Vec<ComponentValue>),
    Variant {
        tag: String,
        value: Option<Box<ComponentValue>>,
    },
    Option(Option<Box<ComponentValue>>),
    Result {
        ok: Option<Box<ComponentValue>>,
        err: Option<Box<ComponentValue>>,
    },
    Enum(String),
    Flags(Vec<String>),
    Handle(u32),
    Unit,
}

impl ComponentValue {
    pub fn as_i32(&self) -> Option<i32> {
        match self {
            ComponentValue::S32(v) => Some(*v),
            ComponentValue::U32(v) => Some(*v as i32),
            ComponentValue::S16(v) => Some(*v as i32),
            ComponentValue::U16(v) => Some(*v as i32),
            ComponentValue::S8(v) => Some(*v as i32),
            ComponentValue::U8(v) => Some(*v as i32),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            ComponentValue::String(s) => Some(s),
            ComponentValue::Enum(s) => Some(s),
            _ => None,
        }
    }

    pub fn to_display_string(&self) -> String {
        match self {
            ComponentValue::Bool(v) => v.to_string(),
            ComponentValue::U8(v) => v.to_string(),
            ComponentValue::U16(v) => v.to_string(),
            ComponentValue::U32(v) => v.to_string(),
            ComponentValue::U64(v) => v.to_string(),
            ComponentValue::S8(v) => v.to_string(),
            ComponentValue::S16(v) => v.to_string(),
            ComponentValue::S32(v) => v.to_string(),
            ComponentValue::S64(v) => v.to_string(),
            ComponentValue::F32(v) => v.to_string(),
            ComponentValue::F64(v) => v.to_string(),
            ComponentValue::Char(c) => format!("'{}'", c),
            ComponentValue::String(s) => s.clone(),
            ComponentValue::List(items) => {
                let rendered: Vec<String> =
                    items.iter().map(|item| item.to_display_string()).collect();
                format!("[{}]", rendered.join(", "))
            }
            ComponentValue::Record(fields) => {
                let rendered: Vec<String> = fields
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, v.to_display_string()))
                    .collect();
                format!("{{{}}}", rendered.join(", "))
            }
            ComponentValue::Tuple(items) => {
                if items.is_empty() {
                    "()".to_string()
                } else {
                    let rendered: Vec<String> =
                        items.iter().map(|item| item.to_display_string()).collect();
                    format!("({})", rendered.join(", "))
                }
            }
            ComponentValue::Variant { tag, value } => match value {
                Some(inner) => format!("{}({})", tag, inner.to_display_string()),
                None => tag.clone(),
            },
            ComponentValue::Option(inner) => match inner {
                Some(value) => format!("some({})", value.to_display_string()),
                None => "none".to_string(),
            },
            ComponentValue::Result { ok, err } => {
                if let Some(value) = ok.as_ref() {
                    format!("ok({})", value.to_display_string())
                } else if let Some(value) = err.as_ref() {
                    format!("err({})", value.to_display_string())
                } else {
                    "ok".to_string()
                }
            }
            ComponentValue::Enum(tag) => tag.clone(),
            ComponentValue::Flags(flags) => {
                if flags.is_empty() {
                    "flags()".to_string()
                } else {
                    format!("flags({})", flags.join("|"))
                }
            }
            ComponentValue::Handle(handle) => format!("handle({})", handle),
            ComponentValue::Unit => "unit".to_string(),
        }
    }

    pub fn to_json_value(&self) -> serde_json::Value {
        use serde_json::{Map, Number, Value};

        match self {
            ComponentValue::Bool(v) => Value::Bool(*v),
            ComponentValue::U8(v) => Value::Number(Number::from(*v)),
            ComponentValue::U16(v) => Value::Number(Number::from(*v)),
            ComponentValue::U32(v) => Value::Number(Number::from(*v)),
            ComponentValue::U64(v) => Value::Number(Number::from(*v)),
            ComponentValue::S8(v) => Value::Number(Number::from(*v)),
            ComponentValue::S16(v) => Value::Number(Number::from(*v)),
            ComponentValue::S32(v) => Value::Number(Number::from(*v)),
            ComponentValue::S64(v) => Value::Number(Number::from(*v)),
            ComponentValue::F32(v) => Number::from_f64(*v as f64)
                .map(Value::Number)
                .unwrap_or(Value::Null),
            ComponentValue::F64(v) => Number::from_f64(*v).map(Value::Number).unwrap_or(Value::Null),
            ComponentValue::Char(c) => Value::String(c.to_string()),
            ComponentValue::String(s) => Value::String(s.clone()),
            ComponentValue::List(items) => {
                Value::Array(items.iter().map(|item| item.to_json_value()).collect())
            }
            ComponentValue::Record(fields) => {
                let mut map = Map::new();
                for (k, v) in fields {
                    map.insert(k.clone(), v.to_json_value());
                }
                Value::Object(map)
            }
            ComponentValue::Tuple(items) => {
                Value::Array(items.iter().map(|item| item.to_json_value()).collect())
            }
            ComponentValue::Variant { tag, value } => {
                let mut map = Map::new();
                map.insert("tag".to_string(), Value::String(tag.clone()));
                map.insert(
                    "value".to_string(),
                    value.as_ref().map(|v| v.to_json_value()).unwrap_or(Value::Null),
                );
                Value::Object(map)
            }
            ComponentValue::Option(inner) => {
                inner.as_ref().map(|v| v.to_json_value()).unwrap_or(Value::Null)
            }
            ComponentValue::Result { ok, err } => {
                let mut map = Map::new();
                map.insert(
                    "ok".to_string(),
                    ok.as_ref().map(|v| v.to_json_value()).unwrap_or(Value::Null),
                );
                map.insert(
                    "err".to_string(),
                    err.as_ref().map(|v| v.to_json_value()).unwrap_or(Value::Null),
                );
                Value::Object(map)
            }
            ComponentValue::Enum(tag) => Value::String(tag.clone()),
            ComponentValue::Flags(flags) => {
                Value::Array(flags.iter().map(|f| Value::String(f.clone())).collect())
            }
            ComponentValue::Handle(handle) => Value::Number(Number::from(*handle)),
            ComponentValue::Unit => Value::Null,
        }
    }

    pub fn parse(raw: &str) -> Result<Self> {
        let raw = raw.trim();
        if raw == "unit" || raw == "()" {
            return Ok(ComponentValue::Unit);
        }

        if raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2 {
            return Ok(ComponentValue::String(
                raw.trim_matches('"').to_string(),
            ));
        }

        if raw == "true" || raw == "false" {
            return Ok(ComponentValue::Bool(raw == "true"));
        }

        if let Some((ty, value)) = raw.split_once(':') {
            return match ty {
                "bool" => Ok(ComponentValue::Bool(value == "true")),
                "u8" => Ok(ComponentValue::U8(value.parse().map_err(map_value_err)?)),
                "u16" => Ok(ComponentValue::U16(value.parse().map_err(map_value_err)?)),
                "u32" => Ok(ComponentValue::U32(value.parse().map_err(map_value_err)?)),
                "u64" => Ok(ComponentValue::U64(value.parse().map_err(map_value_err)?)),
                "s8" => Ok(ComponentValue::S8(value.parse().map_err(map_value_err)?)),
                "s16" => Ok(ComponentValue::S16(value.parse().map_err(map_value_err)?)),
                "s32" => Ok(ComponentValue::S32(value.parse().map_err(map_value_err)?)),
                "s64" => Ok(ComponentValue::S64(value.parse().map_err(map_value_err)?)),
                "f32" => Ok(ComponentValue::F32(value.parse().map_err(map_value_err)?)),
                "f64" => Ok(ComponentValue::F64(value.parse().map_err(map_value_err)?)),
                "char" => Ok(ComponentValue::Char(
                    value
                        .chars()
                        .next()
                        .ok_or_else(|| Error::other("char value is empty"))?,
                )),
                "string" => Ok(ComponentValue::String(value.to_string())),
                "enum" => Ok(ComponentValue::Enum(value.to_string())),
                "flags" => {
                    let parts: Vec<String> = value
                        .split(|c| c == '|' || c == ',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    Ok(ComponentValue::Flags(parts))
                }
                _ => Err(Error::other(format!("Unknown value type '{}'", ty))),
            };
        }

        if let Ok(int_val) = raw.parse::<i64>() {
            if int_val >= i32::MIN as i64 && int_val <= i32::MAX as i64 {
                return Ok(ComponentValue::S32(int_val as i32));
            }
            return Ok(ComponentValue::S64(int_val));
        }

        if let Ok(float_val) = raw.parse::<f64>() {
            return Ok(ComponentValue::F64(float_val));
        }

        Ok(ComponentValue::String(raw.to_string()))
    }
}

fn map_value_err<E: std::fmt::Display>(err: E) -> Error {
    Error::other(format!("Invalid value: {}", err))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_component_value_conversion() {
        let val = ComponentValue::S32(42);
        assert_eq!(val.as_i32(), Some(42));

        let str_val = ComponentValue::String("hello".to_string());
        assert_eq!(str_val.as_string(), Some("hello"));
    }

    #[test]
    fn test_component_value_parse_numbers() {
        match ComponentValue::parse("42").unwrap() {
            ComponentValue::S32(v) => assert_eq!(v, 42),
            other => panic!("unexpected value: {:?}", other),
        }

        match ComponentValue::parse("3.14").unwrap() {
            ComponentValue::F64(v) => assert!((v - 3.14).abs() < 1e-9),
            other => panic!("unexpected value: {:?}", other),
        }

        match ComponentValue::parse("\"hello\"").unwrap() {
            ComponentValue::String(v) => assert_eq!(v, "hello"),
            other => panic!("unexpected value: {:?}", other),
        }
    }

    #[test]
    fn test_component_value_display_and_json() {
        let value = ComponentValue::Record(vec![
            ("sum".to_string(), ComponentValue::S32(3)),
            ("ok".to_string(), ComponentValue::Bool(true)),
        ]);
        let rendered = value.to_display_string();
        assert!(rendered.contains("sum: 3"));
        assert!(rendered.contains("ok: true"));

        let json = value.to_json_value();
        assert_eq!(json["sum"], serde_json::Value::Number(3.into()));
        assert_eq!(json["ok"], serde_json::Value::Bool(true));
    }
}
