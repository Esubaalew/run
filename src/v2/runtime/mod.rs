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
pub use engine::{CliContext, RuntimeConfig, RuntimeEngine};
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

    pub fn parse(raw: &str) -> Result<Self> {
        let raw = raw.trim();
        if raw == "unit" || raw == "()" {
            return Ok(ComponentValue::Unit);
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
}
