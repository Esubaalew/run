//! WIT Parser
//!
//! Parses WIT (WebAssembly Interface Types) definitions.
//! This is a simplified parser - in production, we'd use the wit-parser crate.

use super::*;
use crate::v2::{Error, Result};
use std::collections::HashMap;
use std::path::Path;

pub struct WitParser;

impl WitParser {
    pub fn parse(source: &str) -> Result<WitPackage> {
        let mut parser = WitParserState::new(source);
        parser.parse_package()
    }

    pub fn parse_dir(path: &Path) -> Result<WitPackage> {
        let mut combined = String::new();

        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let file_path = entry.path();
            if file_path.extension().map_or(false, |e| e == "wit") {
                let content = std::fs::read_to_string(&file_path)?;
                combined.push_str(&content);
                combined.push('\n');
            }
        }

        Self::parse(&combined)
    }
}

struct WitParserState<'a> {
    source: &'a str,
    pos: usize,
    line: usize,
    col: usize,
}

impl<'a> WitParserState<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    fn parse_package(&mut self) -> Result<WitPackage> {
        self.skip_whitespace_and_comments();

        let id = self.parse_package_decl()?;

        let mut interfaces = HashMap::new();
        let mut worlds = HashMap::new();

        while !self.is_eof() {
            self.skip_whitespace_and_comments();
            if self.is_eof() {
                break;
            }

            if self.peek_keyword("interface") {
                let iface = self.parse_interface()?;
                interfaces.insert(iface.name.clone(), iface);
            } else if self.peek_keyword("world") {
                let world = self.parse_world()?;
                worlds.insert(world.name.clone(), world);
            } else {
                self.advance();
            }
        }

        Ok(WitPackage {
            id,
            interfaces,
            worlds,
        })
    }

    fn parse_package_decl(&mut self) -> Result<WitPackageId> {
        if !self.consume_keyword("package") {
            return WitPackageId::new("local", "unnamed", None);
        }

        self.skip_whitespace();
        let namespace = self.parse_identifier()?;
        self.expect_char(':')?;
        let name = self.parse_identifier()?;

        let version = if self.peek_char() == Some('@') {
            self.advance(); // consume '@'
            Some(self.parse_version()?)
        } else {
            None
        };

        self.expect_char(';')?;

        WitPackageId::new(&namespace, &name, version.as_deref())
    }

    fn parse_interface(&mut self) -> Result<WitInterface> {
        self.consume_keyword("interface");
        self.skip_whitespace();

        let name = self.parse_identifier()?;
        self.skip_whitespace();
        self.expect_char('{')?;

        let mut types = HashMap::new();
        let mut functions = HashMap::new();
        let docs = None;

        while !self.peek_char_is('}') {
            self.skip_whitespace_and_comments();
            if self.peek_char_is('}') {
                break;
            }

            if self.peek_keyword("type")
                || self.peek_keyword("record")
                || self.peek_keyword("variant")
                || self.peek_keyword("enum")
                || self.peek_keyword("flags")
                || self.peek_keyword("resource")
            {
                let (type_name, ty) = self.parse_type_def()?;
                types.insert(type_name, ty);
            } else {
                let func = self.parse_function()?;
                functions.insert(func.name.clone(), func);
            }
        }

        self.expect_char('}')?;

        Ok(WitInterface {
            name,
            types,
            functions,
            docs,
        })
    }

    fn parse_world(&mut self) -> Result<WitWorld> {
        self.consume_keyword("world");
        self.skip_whitespace();

        let name = self.parse_identifier()?;
        self.skip_whitespace();
        self.expect_char('{')?;

        let mut imports = Vec::new();
        let mut exports = Vec::new();

        while !self.peek_char_is('}') {
            self.skip_whitespace_and_comments();
            if self.peek_char_is('}') {
                break;
            }

            if self.peek_keyword("import") {
                self.consume_keyword("import");
                let item = self.parse_world_item()?;
                imports.push(item);
            } else if self.peek_keyword("export") {
                self.consume_keyword("export");
                let item = self.parse_world_item()?;
                exports.push(item);
            } else {
                self.advance();
            }
        }

        self.expect_char('}')?;

        Ok(WitWorld {
            name,
            imports,
            exports,
            docs: None,
        })
    }

    fn parse_world_item(&mut self) -> Result<WitWorldItem> {
        self.skip_whitespace();
        let name = self.parse_identifier()?;
        self.skip_whitespace();

        if self.peek_char_is(':') {
            self.expect_char(':')?;
            self.skip_whitespace();

            if self.peek_keyword("interface") {
                self.consume_keyword("interface");
                self.skip_whitespace();
                self.expect_char('{')?;
                let mut depth = 1;
                while depth > 0 {
                    match self.advance() {
                        Some('{') => depth += 1,
                        Some('}') => depth -= 1,
                        None => break,
                        _ => {}
                    }
                }
                return Ok(WitWorldItem::Interface {
                    name,
                    interface: WitInterfaceRef::Local("inline".to_string()),
                });
            }

            let interface_ref = self.parse_interface_ref()?;
            self.expect_char(';')?;

            Ok(WitWorldItem::Interface {
                name,
                interface: interface_ref,
            })
        } else {
            self.expect_char(';')?;
            Ok(WitWorldItem::Interface {
                name: name.clone(),
                interface: WitInterfaceRef::Local(name),
            })
        }
    }

    fn parse_interface_ref(&mut self) -> Result<WitInterfaceRef> {
        let first = self.parse_identifier()?;

        if self.peek_char_is(':') {
            self.expect_char(':')?;
            let package_name = self.parse_identifier()?;
            self.skip_whitespace();

            let version = if self.peek_char() == Some('@') {
                self.advance();
                Some(self.parse_version()?)
            } else {
                None
            };
            self.skip_whitespace();

            self.expect_char('/')?;
            self.skip_whitespace();

            let mut interface = self.parse_identifier()?;
            // Support nested interface paths like foo/bar
            while self.peek_char_is('/') {
                self.advance();
                let segment = self.parse_identifier()?;
                interface = format!("{}/{}", interface, segment);
            }

            Ok(WitInterfaceRef::External {
                package: WitPackageId::new(&first, &package_name, version.as_deref())?,
                interface,
            })
        } else {
            Ok(WitInterfaceRef::Local(first))
        }
    }

    fn parse_function(&mut self) -> Result<WitFunction> {
        let name = self.parse_identifier()?;
        self.skip_whitespace();
        self.expect_char(':')?;
        self.skip_whitespace();

        if !self.consume_keyword("func") {
            return Err(Error::other(format!(
                "Expected 'func' keyword at line {}",
                self.line
            )));
        }

        self.skip_whitespace();
        self.expect_char('(')?;

        let mut params = Vec::new();
        while !self.peek_char_is(')') {
            self.skip_whitespace();
            if self.peek_char_is(')') {
                break;
            }

            let param_name = self.parse_identifier()?;
            self.skip_whitespace();
            self.expect_char(':')?;
            self.skip_whitespace();
            let param_type = self.parse_type()?;
            params.push(WitParam {
                name: param_name,
                ty: param_type,
            });

            self.skip_whitespace();
            if self.peek_char_is(',') {
                self.advance();
            }
        }
        self.expect_char(')')?;

        self.skip_whitespace();
        let results = if self.peek_str("->") {
            self.advance();
            self.advance();
            self.skip_whitespace();
            let ret_type = self.parse_type()?;
            WitResults::Anon(ret_type)
        } else {
            WitResults::None
        };

        self.skip_whitespace();
        self.expect_char(';')?;

        Ok(WitFunction {
            name,
            params,
            results,
            docs: None,
        })
    }

    fn parse_type_def(&mut self) -> Result<(String, WitType)> {
        if self.peek_keyword("record") {
            return self.parse_record_def();
        } else if self.peek_keyword("variant") {
            return self.parse_variant_def();
        } else if self.peek_keyword("enum") {
            return self.parse_enum_def();
        } else if self.peek_keyword("flags") {
            return self.parse_flags_def();
        } else if self.peek_keyword("resource") {
            return self.parse_resource_def();
        } else if self.peek_keyword("type") {
            return self.parse_type_alias();
        }

        Err(Error::other(format!(
            "Unknown type definition at line {}",
            self.line
        )))
    }

    fn parse_record_def(&mut self) -> Result<(String, WitType)> {
        self.consume_keyword("record");
        self.skip_whitespace();
        let name = self.parse_identifier()?;
        self.skip_whitespace();
        self.expect_char('{')?;

        let mut fields = Vec::new();
        while !self.peek_char_is('}') {
            self.skip_whitespace_and_comments();
            if self.peek_char_is('}') {
                break;
            }

            let field_name = self.parse_identifier()?;
            self.skip_whitespace();
            self.expect_char(':')?;
            self.skip_whitespace();
            let field_type = self.parse_type()?;
            fields.push(WitField {
                name: field_name,
                ty: field_type,
            });

            self.skip_whitespace();
            if self.peek_char_is(',') {
                self.advance();
            }
        }
        self.expect_char('}')?;

        Ok((name, WitType::Record { fields }))
    }

    fn parse_variant_def(&mut self) -> Result<(String, WitType)> {
        self.consume_keyword("variant");
        self.skip_whitespace();
        let name = self.parse_identifier()?;
        self.skip_whitespace();
        self.expect_char('{')?;

        let mut cases = Vec::new();
        while !self.peek_char_is('}') {
            self.skip_whitespace_and_comments();
            if self.peek_char_is('}') {
                break;
            }

            let case_name = self.parse_identifier()?;
            self.skip_whitespace();

            let case_type = if self.peek_char_is('(') {
                self.advance();
                let ty = self.parse_type()?;
                self.expect_char(')')?;
                Some(ty)
            } else {
                None
            };

            cases.push(WitCase {
                name: case_name,
                ty: case_type,
            });

            self.skip_whitespace();
            if self.peek_char_is(',') {
                self.advance();
            }
        }
        self.expect_char('}')?;

        Ok((name, WitType::Variant { cases }))
    }

    fn parse_enum_def(&mut self) -> Result<(String, WitType)> {
        self.consume_keyword("enum");
        self.skip_whitespace();
        let name = self.parse_identifier()?;
        self.skip_whitespace();
        self.expect_char('{')?;

        let mut cases = Vec::new();
        while !self.peek_char_is('}') {
            self.skip_whitespace_and_comments();
            if self.peek_char_is('}') {
                break;
            }

            let case_name = self.parse_identifier()?;
            cases.push(case_name);

            self.skip_whitespace();
            if self.peek_char_is(',') {
                self.advance();
            }
        }
        self.expect_char('}')?;

        Ok((name, WitType::Enum { cases }))
    }

    fn parse_flags_def(&mut self) -> Result<(String, WitType)> {
        self.consume_keyword("flags");
        self.skip_whitespace();
        let name = self.parse_identifier()?;
        self.skip_whitespace();
        self.expect_char('{')?;

        let mut flags = Vec::new();
        while !self.peek_char_is('}') {
            self.skip_whitespace_and_comments();
            if self.peek_char_is('}') {
                break;
            }

            let flag_name = self.parse_identifier()?;
            flags.push(flag_name);

            self.skip_whitespace();
            if self.peek_char_is(',') {
                self.advance();
            }
        }
        self.expect_char('}')?;

        Ok((name, WitType::Flags { flags }))
    }

    fn parse_resource_def(&mut self) -> Result<(String, WitType)> {
        self.consume_keyword("resource");
        self.skip_whitespace();
        let name = self.parse_identifier()?;
        self.skip_whitespace();

        if self.peek_char_is('{') {
            let mut depth = 1;
            self.advance();
            while depth > 0 {
                match self.advance() {
                    Some('{') => depth += 1,
                    Some('}') => depth -= 1,
                    None => break,
                    _ => {}
                }
            }
        } else {
            self.expect_char(';')?;
        }

        Ok((name.clone(), WitType::Resource { name }))
    }

    fn parse_type_alias(&mut self) -> Result<(String, WitType)> {
        self.consume_keyword("type");
        self.skip_whitespace();
        let name = self.parse_identifier()?;
        self.skip_whitespace();
        self.expect_char('=')?;
        self.skip_whitespace();
        let ty = self.parse_type()?;
        self.expect_char(';')?;

        Ok((name, ty))
    }

    fn parse_type(&mut self) -> Result<WitType> {
        self.skip_whitespace();

        if self.peek_keyword("list") {
            self.consume_keyword("list");
            self.expect_char('<')?;
            let inner = self.parse_type()?;
            self.expect_char('>')?;
            return Ok(WitType::List(Box::new(inner)));
        }

        if self.peek_keyword("option") {
            self.consume_keyword("option");
            self.expect_char('<')?;
            let inner = self.parse_type()?;
            self.expect_char('>')?;
            return Ok(WitType::Option(Box::new(inner)));
        }

        if self.peek_keyword("result") {
            self.consume_keyword("result");
            if self.peek_char_is('<') {
                self.expect_char('<')?;
                let ok = if !self.peek_char_is(',') && !self.peek_char_is('>') {
                    Some(Box::new(self.parse_type()?))
                } else {
                    None
                };

                let err = if self.peek_char_is(',') {
                    self.advance();
                    self.skip_whitespace();
                    if !self.peek_char_is('>') {
                        Some(Box::new(self.parse_type()?))
                    } else {
                        None
                    }
                } else {
                    None
                };

                self.expect_char('>')?;
                return Ok(WitType::Result { ok, err });
            }
            return Ok(WitType::Result {
                ok: None,
                err: None,
            });
        }

        if self.peek_keyword("tuple") {
            self.consume_keyword("tuple");
            self.expect_char('<')?;
            let mut types = Vec::new();
            while !self.peek_char_is('>') {
                self.skip_whitespace();
                types.push(self.parse_type()?);
                self.skip_whitespace();
                if self.peek_char_is(',') {
                    self.advance();
                }
            }
            self.expect_char('>')?;
            return Ok(WitType::Tuple(types));
        }

        if self.peek_keyword("own") {
            self.consume_keyword("own");
            self.expect_char('<')?;
            let name = self.parse_identifier()?;
            self.expect_char('>')?;
            return Ok(WitType::Own(name));
        }
        if self.peek_keyword("borrow") {
            self.consume_keyword("borrow");
            self.expect_char('<')?;
            let name = self.parse_identifier()?;
            self.expect_char('>')?;
            return Ok(WitType::Borrow(name));
        }

        let primitives = [
            ("bool", WitType::Bool),
            ("u8", WitType::U8),
            ("u16", WitType::U16),
            ("u32", WitType::U32),
            ("u64", WitType::U64),
            ("s8", WitType::S8),
            ("s16", WitType::S16),
            ("s32", WitType::S32),
            ("s64", WitType::S64),
            ("f32", WitType::F32),
            ("f64", WitType::F64),
            ("char", WitType::Char),
            ("string", WitType::String),
        ];

        for (name, ty) in primitives {
            if self.peek_keyword(name) {
                self.consume_keyword(name);
                return Ok(ty);
            }
        }

        let name = self.parse_identifier()?;
        Ok(WitType::Named(name))
    }

    fn parse_identifier(&mut self) -> Result<String> {
        self.skip_whitespace();
        let start = self.pos;

        while let Some(c) = self.peek_char() {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                self.advance();
            } else {
                break;
            }
        }

        if self.pos == start {
            return Err(Error::other(format!(
                "Expected identifier at line {}",
                self.line
            )));
        }

        Ok(self.source[start..self.pos].to_string())
    }

    fn parse_version(&mut self) -> Result<String> {
        let start = self.pos;
        while let Some(c) = self.peek_char() {
            if c.is_ascii_digit() || c == '.' || c == '-' || c.is_ascii_alphabetic() {
                self.advance();
            } else {
                break;
            }
        }
        Ok(self.source[start..self.pos].to_string())
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.source.len()
    }

    fn peek_char(&self) -> Option<char> {
        self.source[self.pos..].chars().next()
    }

    fn peek_char_is(&self, c: char) -> bool {
        self.peek_char() == Some(c)
    }

    fn peek_str(&self, s: &str) -> bool {
        self.source[self.pos..].starts_with(s)
    }

    fn peek_keyword(&self, kw: &str) -> bool {
        if !self.source[self.pos..].starts_with(kw) {
            return false;
        }
        let after = self.pos + kw.len();
        if after >= self.source.len() {
            return true;
        }
        let next_char = self.source[after..].chars().next();
        !next_char
            .map(|c| c.is_alphanumeric() || c == '_')
            .unwrap_or(false)
    }

    fn consume_keyword(&mut self, kw: &str) -> bool {
        if self.peek_keyword(kw) {
            self.pos += kw.len();
            true
        } else {
            false
        }
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.peek_char()?;
        self.pos += c.len_utf8();
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn expect_char(&mut self, expected: char) -> Result<()> {
        self.skip_whitespace();
        match self.advance() {
            Some(c) if c == expected => Ok(()),
            Some(c) => Err(Error::other(format!(
                "Expected '{}', found '{}' at line {}",
                expected, c, self.line
            ))),
            None => Err(Error::other(format!(
                "Expected '{}', found EOF at line {}",
                expected, self.line
            ))),
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek_char() {
            if c.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            self.skip_whitespace();
            if self.peek_str("//") {
                while let Some(c) = self.advance() {
                    if c == '\n' {
                        break;
                    }
                }
            } else if self.peek_str("/*") {
                self.advance();
                self.advance();
                while !self.is_eof() && !self.peek_str("*/") {
                    self.advance();
                }
                if self.peek_str("*/") {
                    self.advance();
                    self.advance();
                }
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_interface() {
        let wit = r#"
            package example:math@1.0.0;

            interface calculator {
                add: func(a: s32, b: s32) -> s32;
                multiply: func(a: s32, b: s32) -> s32;
            }
        "#;

        let package = WitParser::parse(wit).unwrap();
        assert_eq!(package.id.namespace, "example");
        assert_eq!(package.id.name, "math");

        let calc = package.get_interface("calculator").unwrap();
        assert!(calc.has_function("add"));
        assert!(calc.has_function("multiply"));
    }

    #[test]
    fn test_parse_world() {
        let wit = r#"
            package example:app@1.0.0;

            world my-app {
                import fs: wasi:filesystem@0.2.0/types;
                export api: interface {
                    run: func() -> result;
                }
            }
        "#;

        let package = WitParser::parse(wit).unwrap();
        let world = package.get_world("my-app").unwrap();
        assert!(!world.imports.is_empty());
        assert!(!world.exports.is_empty());
    }
}
