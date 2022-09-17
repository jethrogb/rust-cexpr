// (C) Copyright 2016 Jethro G. Beekman
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.
extern crate cexpr;

use std::collections::HashMap;
use std::io::Write;
use std::str::{self, FromStr};
use std::char;
use std::num::Wrapping;

use cexpr::assert_full_parse;
use cexpr::expr::{fn_macro_declaration, EvalResult, IdentifierParser};
use cexpr::literal::CChar;
use cexpr::token::Token;
use clang::{source::SourceRange, token::TokenKind, EntityKind};

// main testing routine
fn test_definition(
    ident: Vec<u8>,
    tokens: &[Token],
    idents: &mut HashMap<Vec<u8>, EvalResult>,
) -> bool {
    use cexpr::expr::EvalResult::*;

    fn bytes_to_int(value: &[u8]) -> Option<EvalResult> {
        let s = str::from_utf8(value).ok()?;
        let s = s.rsplit_once('_').map(|(_, s)| s).unwrap_or(s);

        i64::from_str(&s.replace("n", "-")).ok()
            .map(Wrapping)
            .map(Int)
    }

    let display_name = String::from_utf8_lossy(&ident).into_owned();

    let functional;
    let test = {
        // Split name such as Str_test_string into (Str,test_string)
        let pos = ident
            .iter()
            .position(|c| *c == b'_')
            .expect(&format!("Invalid definition in testcase: {}", display_name));
        let mut expected = &ident[..pos];
        let mut value = &ident[(pos + 1)..];

        functional = expected == b"Fn";

        if functional {
            let ident = value;
            let pos = ident
                .iter()
                .position(|c| *c == b'_')
                .expect(&format!("Invalid definition in testcase: {}", display_name));
            expected = &ident[..pos];
            value = &ident[(pos + 1)..];
        }

        if expected == b"Str" {
            let mut splits = value.split(|c| *c == b'U');
            let mut s = Vec::with_capacity(value.len());
            s.extend_from_slice(splits.next().unwrap());
            for split in splits {
                let (chr, rest) = split.split_at(6);
                let chr = u32::from_str_radix(str::from_utf8(chr).unwrap(), 16).unwrap();
                write!(s, "{}", char::from_u32(chr).unwrap()).unwrap();
                s.extend_from_slice(rest);
            }
            Some(Str(s))
        } else if expected == b"Cast" {
            str::from_utf8(value).ok().and_then(|s| {
                let (ty, value) = s.rsplit_once("_Int_")?;

                let ty = ty.split("_").filter_map(|t| {
                    if t == "const" || t == "signed" {
                        None
                    } else {
                      Some(t.as_bytes().to_vec())
                    }
                }).collect::<Vec<Vec<u8>>>();
                let int = bytes_to_int(value.as_bytes())?;

                Some(Cast(ty, Box::new(int)))
            })
        } else if expected == b"Int" {
            bytes_to_int(value)
        } else if expected == b"Float" {
            str::from_utf8(value)
                .ok()
                .map(|s| s.replace("n", "-").replace("p", "."))
                .and_then(|v| f64::from_str(&v).ok())
                .map(Float)
        } else if expected == b"CharRaw" {
            str::from_utf8(value)
                .ok()
                .and_then(|v| u64::from_str(v).ok())
                .map(CChar::Raw)
                .map(Char)
        } else if expected == b"CharChar" {
            str::from_utf8(value)
                .ok()
                .and_then(|v| u32::from_str(v).ok())
                .and_then(char::from_u32)
                .map(CChar::Char)
                .map(Char)
        } else {
            Some(Invalid)
        }
        .expect(&format!("Invalid definition in testcase: {}", display_name))
    };

    let result = if functional {
        let mut fnidents;
        let expr_tokens;
        match fn_macro_declaration(&tokens) {
            Ok((rest, (_, args))) => {
                fnidents = idents.clone();
                expr_tokens = rest;
                for arg in args {
                    let val = match test {
                        Int(_) => bytes_to_int(&arg),
                        Str(_) => Some(Str(arg.to_owned())),
                        _ => unimplemented!(),
                    }
                    .expect(&format!(
                        "Invalid argument in functional macro testcase: {}",
                        display_name
                    ));
                    fnidents.insert(arg.to_owned(), val);
                }
            }
            e => {
                println!(
                    "Failed test for {}, unable to parse functional macro declaration: {:?}",
                    display_name, e
                );
                return false;
            }
        }

        assert_full_parse(IdentifierParser::new(&fnidents).expr(&expr_tokens))
    } else {
        IdentifierParser::new(idents)
            .macro_definition(&tokens)
            .map(|(i, (_, val))| (i, val))
    };

    match result {
        Ok((_, val)) => {
            if val == test {
                if let Some(_) = idents.insert(ident, val) {
                    panic!("Duplicate definition for testcase: {}", display_name);
                }
                true
            } else {
                println!(
                    "Failed test for {}, expected {:?}, got {:?}",
                    display_name, test, val
                );
                false
            }
        }
        e => {
            if test == Invalid {
                true
            } else {
                println!(
                    "Failed test for {}, expected {:?}, got {:?}",
                    display_name, test, e
                );
                false
            }
        }
    }
}

fn token_clang_to_cexpr(token: &clang::token::Token) -> Token {
    Token {
        kind: match token.get_kind() {
            TokenKind::Comment => cexpr::token::Kind::Comment,
            TokenKind::Identifier => cexpr::token::Kind::Identifier,
            TokenKind::Keyword => cexpr::token::Kind::Keyword,
            TokenKind::Literal => cexpr::token::Kind::Literal,
            TokenKind::Punctuation => cexpr::token::Kind::Punctuation,
        },
        raw: token.get_spelling().into_bytes().into_boxed_slice(),
    }
}

fn location_in_scope(r: &SourceRange) -> bool {
    let start = r.get_start();
    let location = start.get_spelling_location();
    start.is_in_main_file() && !start.is_in_system_header() && location.file.is_some()
}

fn file_visit_macros<F: FnMut(Vec<u8>, Vec<Token>)>(
    file: &str,
    mut visitor: F,
) {
    let clang = clang::Clang::new().unwrap();

    let index = clang::Index::new(&clang, false, true);

    let tu = index
        .parser(file)
        .arguments(&["-std=c11"])
        .detailed_preprocessing_record(true)
        .skip_function_bodies(true)
        .parse()
        .unwrap();

    let entity = tu.get_entity();

    entity.visit_children(|cur, _parent| {
        if cur.get_kind() == EntityKind::MacroDefinition {
            let range = cur.get_range().unwrap();
            if !location_in_scope(&range) {
                return clang::EntityVisitResult::Continue;
            }

            let tokens: Vec<_> = range
                .tokenize()
                .into_iter()
                .filter_map(|token| {
                    if token.get_kind() == TokenKind::Comment {
                        return None;
                    }

                    Some(token_clang_to_cexpr(&token))
                })
                .collect();

            let display_name = cur.get_display_name().unwrap();
            visitor(display_name.into_bytes(), tokens)
        }

        clang::EntityVisitResult::Continue
    });
}

fn test_file(file: &str) -> bool {
    let mut idents = HashMap::new();
    let mut all_succeeded = true;
    file_visit_macros(file, |ident, tokens| {
        all_succeeded &= test_definition(ident, &tokens, &mut idents)
    });
    all_succeeded
}

macro_rules! test_file {
    ($f:ident) => {
        #[test]
        fn $f() {
            assert!(
                test_file(concat!("tests/input/", stringify!($f), ".h")),
                "test_file"
            )
        }
    };
}

test_file!(floats);
test_file!(chars);
test_file!(strings);
test_file!(int_signed);
test_file!(int_unsigned);
test_file!(fail);
