//! WIT Extractor
//!
//! Extracts WIT interface information from compiled WASM components.
//! Uses wasm-tools under the hood.

use super::{
    WitCase, WitField, WitFunction, WitInterface, WitInterfaceRef, WitPackage, WitPackageId,
    WitParam, WitResults, WitType, WitWorld, WitWorldItem,
};
use crate::v2::{Error, Result};
use indexmap::IndexMap;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use wit_parser::decoding::decode;
use wit_parser::{InterfaceId, PackageId, Resolve, Type, TypeDefKind, TypeId, WorldItem, WorldKey};

pub fn extract_wit(component_path: &Path) -> Result<WitPackage> {
    let bytes = std::fs::read(component_path)?;
    if let Ok(pkg) = extract_wit_from_bytes_pure(&bytes) {
        return Ok(pkg);
    }
    extract_wit_with_wasm_tools(component_path)
}

fn extract_wit_with_wasm_tools(component_path: &Path) -> Result<WitPackage> {
    let output = Command::new("wasm-tools")
        .args(["component", "wit", component_path.to_str().unwrap()])
        .output()
        .map_err(|e| {
            Error::other(format!(
                "Failed to run wasm-tools. Is it installed? Error: {}",
                e
            ))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::other(format!("wasm-tools failed: {}", stderr)));
    }

    let wit_output = String::from_utf8_lossy(&output.stdout);
    parse_wit_output(&wit_output)
}

pub fn extract_wit_from_bytes(bytes: &[u8]) -> Result<WitPackage> {
    if let Ok(pkg) = extract_wit_from_bytes_pure(bytes) {
        return Ok(pkg);
    }
    let temp_dir = tempfile::tempdir()
        .map_err(|e| Error::other(format!("Failed to create temp dir: {}", e)))?;
    let temp_path = temp_dir.path().join("component.wasm");
    std::fs::write(&temp_path, bytes)?;
    extract_wit_with_wasm_tools(&temp_path)
}

fn extract_wit_from_bytes_pure(bytes: &[u8]) -> Result<WitPackage> {
    let decoded = decode(bytes).map_err(|e| Error::other(format!("WIT decode failed: {}", e)))?;
    let resolve = decoded.resolve();
    let package_id = decoded.package();
    wit_package_from_resolve(resolve, package_id)
}

fn wit_package_from_resolve(resolve: &Resolve, package_id: PackageId) -> Result<WitPackage> {
    let package = resolve
        .packages
        .get(package_id)
        .ok_or_else(|| Error::other("WIT package not found in resolve"))?;

    let id = WitPackageId {
        namespace: package.name.namespace.clone(),
        name: package.name.name.clone(),
        version: package.name.version.clone(),
    };

    let mut interfaces = HashMap::new();
    for (name, iface_id) in package.interfaces.iter() {
        let iface = &resolve.interfaces[*iface_id];
        let mut functions = HashMap::new();
        for (func_name, func) in iface.functions.iter() {
            functions.insert(func_name.clone(), convert_function(resolve, func));
        }

        let mut types = HashMap::new();
        for (type_name, type_id) in iface.types.iter() {
            let ty = convert_type_id(resolve, *type_id);
            types.insert(type_name.clone(), ty);
        }

        interfaces.insert(
            name.clone(),
            WitInterface {
                name: name.clone(),
                types,
                functions,
                docs: iface.docs.contents.clone(),
            },
        );
    }

    let mut worlds = HashMap::new();
    for (name, world_id) in package.worlds.iter() {
        let world = &resolve.worlds[*world_id];
        let imports = convert_world_items(resolve, package_id, &world.imports);
        let exports = convert_world_items(resolve, package_id, &world.exports);
        worlds.insert(
            name.clone(),
            WitWorld {
                name: name.clone(),
                imports,
                exports,
                docs: world.docs.contents.clone(),
            },
        );
    }

    Ok(WitPackage {
        id,
        interfaces,
        worlds,
    })
}

fn convert_world_items(
    resolve: &Resolve,
    package_id: PackageId,
    items: &IndexMap<WorldKey, WorldItem>,
) -> Vec<WitWorldItem> {
    let mut results = Vec::new();

    for (key, item) in items {
        let name = world_key_name(resolve, key);
        match item {
            WorldItem::Interface { id, .. } => {
                let interface_ref = convert_interface_ref(resolve, package_id, *id, &name);
                results.push(WitWorldItem::Interface {
                    name,
                    interface: interface_ref,
                });
            }
            WorldItem::Function(func) => {
                results.push(WitWorldItem::Function(convert_function(resolve, func)));
            }
            WorldItem::Type(type_id) => {
                results.push(WitWorldItem::Type {
                    name,
                    ty: convert_type_id(resolve, *type_id),
                });
            }
        }
    }

    results
}

fn world_key_name(resolve: &Resolve, key: &WorldKey) -> String {
    match key {
        WorldKey::Name(name) => name.clone(),
        WorldKey::Interface(id) => resolve.interfaces[*id]
            .name
            .clone()
            .unwrap_or_else(|| format!("interface-{}", id.index())),
    }
}

fn convert_interface_ref(
    resolve: &Resolve,
    current_package: PackageId,
    interface_id: InterfaceId,
    fallback_name: &str,
) -> WitInterfaceRef {
    let interface = &resolve.interfaces[interface_id];
    let name = interface
        .name
        .clone()
        .unwrap_or_else(|| fallback_name.to_string());

    match interface.package {
        Some(pkg_id) if pkg_id != current_package => {
            let pkg = &resolve.packages[pkg_id];
            WitInterfaceRef::External {
                package: WitPackageId {
                    namespace: pkg.name.namespace.clone(),
                    name: pkg.name.name.clone(),
                    version: pkg.name.version.clone(),
                },
                interface: name,
            }
        }
        _ => WitInterfaceRef::Local(name),
    }
}

fn convert_function(resolve: &Resolve, func: &wit_parser::Function) -> WitFunction {
    let params = func
        .params
        .iter()
        .map(|(name, ty)| WitParam {
            name: name.clone(),
            ty: convert_type(resolve, ty),
        })
        .collect();

    let results = match &func.results {
        wit_parser::Results::Named(items) => {
            if items.is_empty() {
                WitResults::None
            } else {
                WitResults::Named(
                    items
                        .iter()
                        .map(|(name, ty)| WitParam {
                            name: name.clone(),
                            ty: convert_type(resolve, ty),
                        })
                        .collect(),
                )
            }
        }
        wit_parser::Results::Anon(ty) => WitResults::Anon(convert_type(resolve, ty)),
    };

    WitFunction {
        name: func.name.clone(),
        params,
        results,
        docs: func.docs.contents.clone(),
    }
}

fn convert_type(resolve: &Resolve, ty: &Type) -> WitType {
    match ty {
        Type::Bool => WitType::Bool,
        Type::U8 => WitType::U8,
        Type::U16 => WitType::U16,
        Type::U32 => WitType::U32,
        Type::U64 => WitType::U64,
        Type::S8 => WitType::S8,
        Type::S16 => WitType::S16,
        Type::S32 => WitType::S32,
        Type::S64 => WitType::S64,
        Type::F32 => WitType::F32,
        Type::F64 => WitType::F64,
        Type::Char => WitType::Char,
        Type::String => WitType::String,
        Type::Id(id) => convert_type_id(resolve, *id),
    }
}

fn convert_type_id(resolve: &Resolve, type_id: TypeId) -> WitType {
    let mut visiting = std::collections::HashSet::new();
    convert_type_id_inner(resolve, type_id, &mut visiting)
}

fn convert_type_id_inner(
    resolve: &Resolve,
    type_id: TypeId,
    visiting: &mut std::collections::HashSet<TypeId>,
) -> WitType {
    if !visiting.insert(type_id) {
        return WitType::Named("recursive".to_string());
    }

    let def = &resolve.types[type_id];
    if let Some(name) = &def.name {
        visiting.remove(&type_id);
        return WitType::Named(name.clone());
    }

    let result = match &def.kind {
        TypeDefKind::Record(record) => WitType::Record {
            fields: record
                .fields
                .iter()
                .map(|field| WitField {
                    name: field.name.clone(),
                    ty: convert_type(resolve, &field.ty),
                })
                .collect(),
        },
        TypeDefKind::Variant(variant) => WitType::Variant {
            cases: variant
                .cases
                .iter()
                .map(|case| WitCase {
                    name: case.name.clone(),
                    ty: case.ty.as_ref().map(|ty| convert_type(resolve, ty)),
                })
                .collect(),
        },
        TypeDefKind::Enum(enum_) => WitType::Enum {
            cases: enum_.cases.iter().map(|c| c.name.clone()).collect(),
        },
        TypeDefKind::Flags(flags) => WitType::Flags {
            flags: flags.flags.iter().map(|f| f.name.clone()).collect(),
        },
        TypeDefKind::Tuple(tuple) => WitType::Tuple(
            tuple
                .types
                .iter()
                .map(|ty| convert_type(resolve, ty))
                .collect(),
        ),
        TypeDefKind::Option(inner) => WitType::Option(Box::new(convert_type(resolve, inner))),
        TypeDefKind::Result(result) => WitType::Result {
            ok: result
                .ok
                .as_ref()
                .map(|ty| Box::new(convert_type(resolve, ty))),
            err: result
                .err
                .as_ref()
                .map(|ty| Box::new(convert_type(resolve, ty))),
        },
        TypeDefKind::List(inner) => WitType::List(Box::new(convert_type(resolve, inner))),
        TypeDefKind::Type(inner) => convert_type(resolve, inner),
        TypeDefKind::Handle(handle) => match handle {
            wit_parser::Handle::Own(id) => WitType::Own(resource_name(resolve, *id)),
            wit_parser::Handle::Borrow(id) => WitType::Borrow(resource_name(resolve, *id)),
        },
        TypeDefKind::Resource => WitType::Resource {
            name: def.name.clone().unwrap_or_else(|| "resource".to_string()),
        },
        TypeDefKind::Future(_) => WitType::Named("future".to_string()),
        TypeDefKind::Stream(_) => WitType::Named("stream".to_string()),
        TypeDefKind::Unknown => WitType::Named("unknown".to_string()),
    };

    visiting.remove(&type_id);
    result
}

fn resource_name(resolve: &Resolve, type_id: TypeId) -> String {
    resolve.types[type_id]
        .name
        .clone()
        .unwrap_or_else(|| "resource".to_string())
}

fn parse_wit_output(wit: &str) -> Result<WitPackage> {
    let mut package_id = WitPackageId {
        namespace: "root".to_string(),
        name: "component".to_string(),
        version: None,
    };

    let mut interfaces: HashMap<String, WitInterface> = HashMap::new();
    let mut worlds: HashMap<String, WitWorld> = HashMap::new();

    let mut current_interface: Option<(String, WitInterface)> = None;
    let mut current_world: Option<(String, WitWorld)> = None;
    let mut in_interface = false;
    let mut in_world = false;
    let mut brace_depth = 0;

    for line in wit.lines() {
        let line = line.trim();

        if line.is_empty() || line.starts_with("//") {
            continue;
        }

        if line.starts_with("package ") {
            if let Some(id) = parse_package_id(line) {
                package_id = id;
            }
            continue;
        }

        brace_depth += line.matches('{').count();
        brace_depth = brace_depth.saturating_sub(line.matches('}').count());

        if line.starts_with("interface ") || line.starts_with("export interface ") {
            if let Some((name, iface)) = current_interface.take() {
                interfaces.insert(name, iface);
            }

            let name = line
                .strip_prefix("interface ")
                .or_else(|| line.strip_prefix("export interface "))
                .and_then(|s| s.split_whitespace().next())
                .unwrap_or("unknown")
                .to_string();

            current_interface = Some((
                name.clone(),
                WitInterface {
                    name: name.clone(),
                    types: HashMap::new(),
                    functions: HashMap::new(),
                    docs: None,
                },
            ));
            in_interface = true;
            in_world = false;
            continue;
        }

        if line.starts_with("world ") {
            if let Some((name, world)) = current_world.take() {
                worlds.insert(name, world);
            }

            let name = line
                .strip_prefix("world ")
                .and_then(|s| s.split_whitespace().next())
                .unwrap_or("unknown")
                .to_string();

            current_world = Some((
                name.clone(),
                WitWorld {
                    name: name.clone(),
                    imports: Vec::new(),
                    exports: Vec::new(),
                    docs: None,
                },
            ));
            in_world = true;
            in_interface = false;
            continue;
        }

        if in_interface {
            if let Some((_, ref mut iface)) = current_interface {
                if let Some(func) = parse_function(line) {
                    iface.functions.insert(func.name.clone(), func);
                }
            }
        }

        if in_world {
            if let Some((_, ref mut world)) = current_world {
                if line.starts_with("import ") {
                    if let Some(item) = parse_world_item(line, true) {
                        world.imports.push(item);
                    }
                } else if line.starts_with("export ") {
                    if let Some(item) = parse_world_item(line, false) {
                        world.exports.push(item);
                    }
                }
            }
        }

        if line == "}" && brace_depth == 0 {
            if let Some((name, iface)) = current_interface.take() {
                interfaces.insert(name, iface);
            }
            if let Some((name, world)) = current_world.take() {
                worlds.insert(name, world);
            }
            in_interface = false;
            in_world = false;
        }
    }

    if let Some((name, iface)) = current_interface {
        interfaces.insert(name, iface);
    }
    if let Some((name, world)) = current_world {
        worlds.insert(name, world);
    }

    Ok(WitPackage {
        id: package_id,
        interfaces,
        worlds,
    })
}

fn parse_package_id(line: &str) -> Option<WitPackageId> {
    let line = line.strip_prefix("package ")?.trim_end_matches(';').trim();

    let (package, version) = if line.contains('@') {
        let parts: Vec<&str> = line.splitn(2, '@').collect();
        (parts[0], Some(parts[1]))
    } else {
        (line, None)
    };

    let parts: Vec<&str> = package.splitn(2, ':').collect();
    if parts.len() == 2 {
        Some(WitPackageId {
            namespace: parts[0].to_string(),
            name: parts[1].to_string(),
            version: version.and_then(|v| semver::Version::parse(v).ok()),
        })
    } else {
        Some(WitPackageId {
            namespace: "local".to_string(),
            name: package.to_string(),
            version: version.and_then(|v| semver::Version::parse(v).ok()),
        })
    }
}

fn parse_function(line: &str) -> Option<WitFunction> {
    if !line.contains(": func(") {
        return None;
    }

    let parts: Vec<&str> = line.splitn(2, ": func(").collect();
    if parts.len() != 2 {
        return None;
    }

    let name = parts[0].trim().to_string();
    let rest = parts[1];

    let (params_str, results_str) = if rest.contains(") -> ") {
        let parts: Vec<&str> = rest.splitn(2, ") -> ").collect();
        (parts[0], Some(parts[1].trim_end_matches(';').trim()))
    } else {
        (rest.trim_end_matches(");").trim(), None)
    };

    let params = parse_params(params_str);
    let results = match results_str {
        Some(r) => WitResults::Anon(parse_type(r)),
        None => WitResults::None,
    };

    Some(WitFunction {
        name,
        params,
        results,
        docs: None,
    })
}

fn parse_params(params_str: &str) -> Vec<WitParam> {
    if params_str.is_empty() {
        return Vec::new();
    }

    params_str
        .split(',')
        .filter_map(|p| {
            let parts: Vec<&str> = p.trim().splitn(2, ": ").collect();
            if parts.len() == 2 {
                Some(WitParam {
                    name: parts[0].trim().to_string(),
                    ty: parse_type(parts[1].trim()),
                })
            } else {
                None
            }
        })
        .collect()
}

fn parse_type(type_str: &str) -> WitType {
    let type_str = type_str.trim();

    match type_str {
        "bool" => WitType::Bool,
        "u8" => WitType::U8,
        "u16" => WitType::U16,
        "u32" => WitType::U32,
        "u64" => WitType::U64,
        "s8" => WitType::S8,
        "s16" => WitType::S16,
        "s32" => WitType::S32,
        "s64" => WitType::S64,
        "f32" => WitType::F32,
        "f64" => WitType::F64,
        "char" => WitType::Char,
        "string" => WitType::String,
        _ if type_str.starts_with("list<") => {
            let inner = type_str
                .strip_prefix("list<")
                .and_then(|s| s.strip_suffix('>'))
                .unwrap_or("u8");
            WitType::List(Box::new(parse_type(inner)))
        }
        _ if type_str.starts_with("option<") => {
            let inner = type_str
                .strip_prefix("option<")
                .and_then(|s| s.strip_suffix('>'))
                .unwrap_or("u8");
            WitType::Option(Box::new(parse_type(inner)))
        }
        _ => WitType::Named(type_str.to_string()),
    }
}

fn parse_world_item(line: &str, is_import: bool) -> Option<WitWorldItem> {
    let line = if is_import {
        line.strip_prefix("import ")?
    } else {
        line.strip_prefix("export ")?
    };

    let line = line.trim_end_matches(';').trim();

    if line.contains(':') || line.contains('/') {
        let interface_ref = parse_interface_ref(line)?;
        let name = line.split('/').last().unwrap_or(line).to_string();
        Some(WitWorldItem::Interface {
            name,
            interface: interface_ref,
        })
    } else {
        Some(WitWorldItem::Interface {
            name: line.to_string(),
            interface: WitInterfaceRef::Local(line.to_string()),
        })
    }
}

fn parse_interface_ref(ref_str: &str) -> Option<WitInterfaceRef> {
    if ref_str.contains('/') {
        let parts: Vec<&str> = ref_str.splitn(2, '/').collect();
        if parts.len() == 2 {
            let package_str = parts[0];
            let interface = parts[1].split('@').next().unwrap_or(parts[1]);
            let version = parts[1].split('@').nth(1);

            let (namespace, name) = if package_str.contains(':') {
                let pp: Vec<&str> = package_str.splitn(2, ':').collect();
                (pp[0], pp[1])
            } else {
                ("local", package_str)
            };

            return Some(WitInterfaceRef::External {
                package: WitPackageId {
                    namespace: namespace.to_string(),
                    name: name.to_string(),
                    version: version.and_then(|v| semver::Version::parse(v).ok()),
                },
                interface: interface.to_string(),
            });
        }
    }

    Some(WitInterfaceRef::Local(ref_str.to_string()))
}

pub fn get_exports(component_path: &Path) -> Result<Vec<String>> {
    let wit = extract_wit(component_path)?;

    let mut exports = Vec::new();
    for world in wit.worlds.values() {
        for export in &world.exports {
            if let WitWorldItem::Interface { name, .. } = export {
                exports.push(name.clone());
            }
        }
    }

    for name in wit.interfaces.keys() {
        if !exports.contains(name) {
            exports.push(name.clone());
        }
    }

    Ok(exports)
}

pub fn get_imports(component_path: &Path) -> Result<Vec<String>> {
    let wit = extract_wit(component_path)?;

    let mut imports = Vec::new();
    for world in wit.worlds.values() {
        for import in &world.imports {
            if let WitWorldItem::Interface { name: _, interface } = import {
                let import_str = match interface {
                    WitInterfaceRef::Local(n) => n.clone(),
                    WitInterfaceRef::External { package, interface } => {
                        format!("{}/{}", package.to_string(), interface)
                    }
                };
                imports.push(import_str);
            }
        }
    }

    Ok(imports)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wit_parser::Resolve;

    #[test]
    fn test_parse_package_id() {
        let id = parse_package_id("package wasi:cli@0.2.3;").unwrap();
        assert_eq!(id.namespace, "wasi");
        assert_eq!(id.name, "cli");
        assert_eq!(id.version, Some(semver::Version::new(0, 2, 3)));
    }

    #[test]
    fn test_parse_function() {
        let func = parse_function("  add: func(a: s32, b: s32) -> s32;").unwrap();
        assert_eq!(func.name, "add");
        assert_eq!(func.params.len(), 2);
        assert_eq!(func.params[0].name, "a");
    }

    #[test]
    fn test_parse_type() {
        assert_eq!(parse_type("s32"), WitType::S32);
        assert_eq!(parse_type("string"), WitType::String);
        assert_eq!(parse_type("list<u8>"), WitType::List(Box::new(WitType::U8)));
    }

    #[test]
    fn test_parse_wit_output() {
        let wit = r#"
package test:calc@0.1.0;

interface calculator {
  add: func(a: s32, b: s32) -> s32;
  subtract: func(a: s32, b: s32) -> s32;
}

world calculator-impl {
  export calculator;
}
"#;

        let package = parse_wit_output(wit).unwrap();
        assert_eq!(package.id.namespace, "test");
        assert_eq!(package.id.name, "calc");
        assert!(package.interfaces.contains_key("calculator"));

        let calc = package.interfaces.get("calculator").unwrap();
        assert!(calc.functions.contains_key("add"));
        assert!(calc.functions.contains_key("subtract"));
    }

    #[test]
    fn test_wit_package_from_resolve() {
        let wit = r#"
package test:calc@0.1.0;

interface calculator {
  add: func(a: s32, b: s32) -> s32;
}

world calculator-impl {
  export calculator;
}
"#;
        let mut resolve = Resolve::default();
        let package_id = resolve.push_str("in-memory.wit", wit).unwrap();
        let package = wit_package_from_resolve(&resolve, package_id).unwrap();
        assert_eq!(package.id.namespace, "test");
        assert_eq!(package.id.name, "calc");
        assert!(package.interfaces.contains_key("calculator"));
        assert!(package.worlds.contains_key("calculator-impl"));
    }
}
