//! WIT (WebAssembly Interface Types) handling
//!
//! This module provides parsing, validation, and code generation for WIT interfaces.
//! WIT is the contract language that enables cross-language component composition.

mod codegen;
mod extractor;
mod parser;
mod resolver;
mod types;

pub use codegen::WitCodegen;
pub use extractor::{extract_wit, extract_wit_from_bytes, get_exports, get_imports};
pub use parser::WitParser;
pub use resolver::WitResolver;
pub use types::*;

use crate::v2::Result;
use std::collections::HashMap;
use std::path::Path;
#[derive(Debug, Clone)]
pub struct WitPackage {
    pub id: WitPackageId,

    pub interfaces: HashMap<String, WitInterface>,

    pub worlds: HashMap<String, WitWorld>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WitPackageId {
    pub namespace: String,
    pub name: String,
    pub version: Option<semver::Version>,
}

impl WitPackageId {
    pub fn new(namespace: &str, name: &str, version: Option<&str>) -> Result<Self> {
        Ok(Self {
            namespace: namespace.to_string(),
            name: name.to_string(),
            version: version
                .map(|v| semver::Version::parse(v))
                .transpose()
                .map_err(|e| crate::v2::Error::other(format!("Invalid version: {}", e)))?,
        })
    }

    pub fn to_string(&self) -> String {
        match &self.version {
            Some(v) => format!("{}:{}@{}", self.namespace, self.name, v),
            None => format!("{}:{}", self.namespace, self.name),
        }
    }
}
#[derive(Debug, Clone)]
pub struct WitInterface {
    pub name: String,

    pub types: HashMap<String, WitType>,

    pub functions: HashMap<String, WitFunction>,

    pub docs: Option<String>,
}
#[derive(Debug, Clone)]
pub struct WitWorld {
    pub name: String,

    pub imports: Vec<WitWorldItem>,

    pub exports: Vec<WitWorldItem>,

    pub docs: Option<String>,
}
#[derive(Debug, Clone)]
pub enum WitWorldItem {
    Interface {
        name: String,
        interface: WitInterfaceRef,
    },

    Function(WitFunction),

    Type {
        name: String,
        ty: WitType,
    },
}
#[derive(Debug, Clone)]
pub enum WitInterfaceRef {
    Local(String),

    External {
        package: WitPackageId,
        interface: String,
    },
}
#[derive(Debug, Clone)]
pub struct WitFunction {
    pub name: String,

    pub params: Vec<WitParam>,

    pub results: WitResults,

    pub docs: Option<String>,
}
#[derive(Debug, Clone)]
pub struct WitParam {
    pub name: String,
    pub ty: WitType,
}
#[derive(Debug, Clone)]
pub enum WitResults {
    None,

    Anon(WitType),

    Named(Vec<WitParam>),
}
#[derive(Debug, Clone, PartialEq)]
pub enum WitType {
    Bool,
    U8,
    U16,
    U32,
    U64,
    S8,
    S16,
    S32,
    S64,
    F32,
    F64,
    Char,
    String,

    List(Box<WitType>),
    Option(Box<WitType>),
    Result {
        ok: Option<Box<WitType>>,
        err: Option<Box<WitType>>,
    },
    Tuple(Vec<WitType>),

    Record {
        fields: Vec<WitField>,
    },
    Variant {
        cases: Vec<WitCase>,
    },
    Enum {
        cases: Vec<String>,
    },
    Flags {
        flags: Vec<String>,
    },

    Resource {
        name: String,
    },

    Named(String),

    Own(String),    // Owned handle
    Borrow(String), // Borrowed handle
}
#[derive(Debug, Clone, PartialEq)]
pub struct WitField {
    pub name: String,
    pub ty: WitType,
}
#[derive(Debug, Clone, PartialEq)]
pub struct WitCase {
    pub name: String,
    pub ty: Option<WitType>,
}

impl WitPackage {
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        WitParser::parse(&content)
    }
    pub fn from_dir(path: &Path) -> Result<Self> {
        WitParser::parse_dir(path)
    }
    pub fn get_interface(&self, name: &str) -> Option<&WitInterface> {
        self.interfaces.get(name)
    }
    pub fn get_world(&self, name: &str) -> Option<&WitWorld> {
        self.worlds.get(name)
    }
    pub fn is_compatible_with(&self, other: &WitPackage) -> bool {
        match (&self.id.version, &other.id.version) {
            (Some(a), Some(b)) => a.major == b.major && a >= b,
            _ => true, // No version = always compatible
        }
    }
}

impl WitInterface {
    pub fn has_function(&self, name: &str) -> bool {
        self.functions.contains_key(name)
    }
    pub fn get_function(&self, name: &str) -> Option<&WitFunction> {
        self.functions.get(name)
    }
}

impl WitType {
    pub fn is_primitive(&self) -> bool {
        matches!(
            self,
            WitType::Bool
                | WitType::U8
                | WitType::U16
                | WitType::U32
                | WitType::U64
                | WitType::S8
                | WitType::S16
                | WitType::S32
                | WitType::S64
                | WitType::F32
                | WitType::F64
                | WitType::Char
                | WitType::String
        )
    }
    pub fn size_hint(&self) -> Option<usize> {
        match self {
            WitType::Bool | WitType::U8 | WitType::S8 => Some(1),
            WitType::U16 | WitType::S16 => Some(2),
            WitType::U32 | WitType::S32 | WitType::F32 | WitType::Char => Some(4),
            WitType::U64 | WitType::S64 | WitType::F64 => Some(8),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_package_id_parsing() {
        let id = WitPackageId::new("wasi", "filesystem", Some("0.2.0")).unwrap();
        assert_eq!(id.namespace, "wasi");
        assert_eq!(id.name, "filesystem");
        assert_eq!(id.version, Some(semver::Version::new(0, 2, 0)));
    }

    #[test]
    fn test_type_primitives() {
        assert!(WitType::Bool.is_primitive());
        assert!(WitType::String.is_primitive());
        assert!(!WitType::List(Box::new(WitType::U8)).is_primitive());
    }
}
