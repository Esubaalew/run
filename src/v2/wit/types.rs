//! WIT Type utilities
//!
//! Helper functions for working with WIT types.

use super::WitType;

pub fn type_to_string(ty: &WitType) -> String {
    match ty {
        WitType::Bool => "bool".to_string(),
        WitType::U8 => "u8".to_string(),
        WitType::U16 => "u16".to_string(),
        WitType::U32 => "u32".to_string(),
        WitType::U64 => "u64".to_string(),
        WitType::S8 => "s8".to_string(),
        WitType::S16 => "s16".to_string(),
        WitType::S32 => "s32".to_string(),
        WitType::S64 => "s64".to_string(),
        WitType::F32 => "f32".to_string(),
        WitType::F64 => "f64".to_string(),
        WitType::Char => "char".to_string(),
        WitType::String => "string".to_string(),
        WitType::List(inner) => format!("list<{}>", type_to_string(inner)),
        WitType::Option(inner) => format!("option<{}>", type_to_string(inner)),
        WitType::Result { ok, err } => match (ok, err) {
            (Some(o), Some(e)) => format!("result<{}, {}>", type_to_string(o), type_to_string(e)),
            (Some(o), None) => format!("result<{}>", type_to_string(o)),
            (None, Some(e)) => format!("result<_, {}>", type_to_string(e)),
            (None, None) => "result".to_string(),
        },
        WitType::Tuple(types) => {
            let inner: Vec<_> = types.iter().map(type_to_string).collect();
            format!("tuple<{}>", inner.join(", "))
        }
        WitType::Record { fields } => {
            let field_strs: Vec<_> = fields
                .iter()
                .map(|f| format!("{}: {}", f.name, type_to_string(&f.ty)))
                .collect();
            format!("record {{ {} }}", field_strs.join(", "))
        }
        WitType::Variant { cases } => {
            let case_strs: Vec<_> = cases
                .iter()
                .map(|c| match &c.ty {
                    Some(ty) => format!("{}({})", c.name, type_to_string(ty)),
                    None => c.name.clone(),
                })
                .collect();
            format!("variant {{ {} }}", case_strs.join(", "))
        }
        WitType::Enum { cases } => {
            format!("enum {{ {} }}", cases.join(", "))
        }
        WitType::Flags { flags } => {
            format!("flags {{ {} }}", flags.join(", "))
        }
        WitType::Resource { name } => format!("resource {}", name),
        WitType::Named(name) => name.clone(),
        WitType::Own(name) => format!("own<{}>", name),
        WitType::Borrow(name) => format!("borrow<{}>", name),
    }
}

pub fn types_compatible(a: &WitType, b: &WitType) -> bool {
    match (a, b) {
        (WitType::Bool, WitType::Bool) => true,
        (WitType::U8, WitType::U8) => true,
        (WitType::U16, WitType::U16) => true,
        (WitType::U32, WitType::U32) => true,
        (WitType::U64, WitType::U64) => true,
        (WitType::S8, WitType::S8) => true,
        (WitType::S16, WitType::S16) => true,
        (WitType::S32, WitType::S32) => true,
        (WitType::S64, WitType::S64) => true,
        (WitType::F32, WitType::F32) => true,
        (WitType::F64, WitType::F64) => true,
        (WitType::Char, WitType::Char) => true,
        (WitType::String, WitType::String) => true,

        (WitType::List(a_inner), WitType::List(b_inner)) => types_compatible(a_inner, b_inner),
        (WitType::Option(a_inner), WitType::Option(b_inner)) => types_compatible(a_inner, b_inner),
        (
            WitType::Result {
                ok: a_ok,
                err: a_err,
            },
            WitType::Result {
                ok: b_ok,
                err: b_err,
            },
        ) => {
            let ok_compat = match (a_ok, b_ok) {
                (Some(a), Some(b)) => types_compatible(a, b),
                (None, None) => true,
                _ => false,
            };
            let err_compat = match (a_err, b_err) {
                (Some(a), Some(b)) => types_compatible(a, b),
                (None, None) => true,
                _ => false,
            };
            ok_compat && err_compat
        }
        (WitType::Tuple(a_types), WitType::Tuple(b_types)) => {
            a_types.len() == b_types.len()
                && a_types
                    .iter()
                    .zip(b_types.iter())
                    .all(|(a, b)| types_compatible(a, b))
        }

        (WitType::Record { fields: a_fields }, WitType::Record { fields: b_fields }) => {
            if a_fields.len() != b_fields.len() {
                return false;
            }
            a_fields.iter().all(|a_field| {
                b_fields.iter().any(|b_field| {
                    a_field.name == b_field.name && types_compatible(&a_field.ty, &b_field.ty)
                })
            })
        }

        (WitType::Named(a_name), WitType::Named(b_name)) => a_name == b_name,

        (WitType::Own(a_name), WitType::Own(b_name)) => a_name == b_name,
        (WitType::Borrow(a_name), WitType::Borrow(b_name)) => a_name == b_name,

        _ => false,
    }
}

pub fn rust_type_to_wit(rust_type: &str) -> Option<WitType> {
    match rust_type {
        "bool" => Some(WitType::Bool),
        "u8" => Some(WitType::U8),
        "u16" => Some(WitType::U16),
        "u32" => Some(WitType::U32),
        "u64" => Some(WitType::U64),
        "i8" => Some(WitType::S8),
        "i16" => Some(WitType::S16),
        "i32" => Some(WitType::S32),
        "i64" => Some(WitType::S64),
        "f32" => Some(WitType::F32),
        "f64" => Some(WitType::F64),
        "char" => Some(WitType::Char),
        "String" | "&str" => Some(WitType::String),
        _ => None,
    }
}

pub fn wit_type_to_rust(wit_type: &WitType) -> String {
    match wit_type {
        WitType::Bool => "bool".to_string(),
        WitType::U8 => "u8".to_string(),
        WitType::U16 => "u16".to_string(),
        WitType::U32 => "u32".to_string(),
        WitType::U64 => "u64".to_string(),
        WitType::S8 => "i8".to_string(),
        WitType::S16 => "i16".to_string(),
        WitType::S32 => "i32".to_string(),
        WitType::S64 => "i64".to_string(),
        WitType::F32 => "f32".to_string(),
        WitType::F64 => "f64".to_string(),
        WitType::Char => "char".to_string(),
        WitType::String => "String".to_string(),
        WitType::List(inner) => format!("Vec<{}>", wit_type_to_rust(inner)),
        WitType::Option(inner) => format!("Option<{}>", wit_type_to_rust(inner)),
        WitType::Result { ok, err } => {
            let ok_type = ok
                .as_ref()
                .map(|t| wit_type_to_rust(t))
                .unwrap_or_else(|| "()".to_string());
            let err_type = err
                .as_ref()
                .map(|t| wit_type_to_rust(t))
                .unwrap_or_else(|| "()".to_string());
            format!("Result<{}, {}>", ok_type, err_type)
        }
        WitType::Tuple(types) => {
            let inner: Vec<_> = types.iter().map(wit_type_to_rust).collect();
            format!("({})", inner.join(", "))
        }
        WitType::Named(name) => to_rust_type_name(name),
        WitType::Own(name) => format!("Own<{}>", to_rust_type_name(name)),
        WitType::Borrow(name) => format!("&{}", to_rust_type_name(name)),
        _ => "/* complex type */".to_string(),
    }
}

fn to_rust_type_name(name: &str) -> String {
    name.split('-')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_to_string() {
        assert_eq!(type_to_string(&WitType::String), "string");
        assert_eq!(
            type_to_string(&WitType::List(Box::new(WitType::U8))),
            "list<u8>"
        );
    }

    #[test]
    fn test_type_compatibility() {
        assert!(types_compatible(&WitType::U32, &WitType::U32));
        assert!(!types_compatible(&WitType::U32, &WitType::S32));

        assert!(types_compatible(
            &WitType::List(Box::new(WitType::String)),
            &WitType::List(Box::new(WitType::String))
        ));
    }

    #[test]
    fn test_wit_to_rust() {
        assert_eq!(wit_type_to_rust(&WitType::S32), "i32");
        assert_eq!(wit_type_to_rust(&WitType::String), "String");
        assert_eq!(
            wit_type_to_rust(&WitType::Option(Box::new(WitType::U64))),
            "Option<u64>"
        );
    }
}
