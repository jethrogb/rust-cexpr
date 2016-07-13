// (C) Copyright 2016 Jethro G. Beekman
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.
//! Parsing C literals from byte slices.
//! 
//! This will parse a representation of a C literal into a Rust type.
//!
//! # characters
//! Character literals are stored into the `CChar` type, which can hold values
//! that are not valid Unicode code points. ASCII characters are represented as
//! `char`, literal bytes with the high byte set are converted into the raw
//! representation. Escape sequences are supported. If hex and octal escapes
//! map to an ASCII character, that is used, otherwise, the raw encoding is
//! used, including for values over 255. Unicode escapes are checked for
//! validity and mapped to `char`. Character sequences are not supported. Width
//! prefixes are ignored.
//!
//! # strings
//! Strings are interpreted as byte vectors. Escape sequences are supported. If
//! hex and octal escapes map onto multi-byte characters, they are truncated to
//! one 8-bit character. Unicode escapes are converted into their UTF-8
//! encoding. Width prefixes are ignored.
//!
//! # integers
//! Integers are read into `i64`. Binary, octal, decimal and hexadecimal are
//! all supported. If the literal value is between `i64::MAX` and `u64::MAX`,
//! it is bit-cast to `i64`. Values over `u64::MAX` cannot be parsed. Width and
//! sign suffixes are ignored. Sign prefixes are not supported.
//!
//! # real numbers
//! Reals are read into `f64`. Width suffixes are ignored. Sign prefixes are
//! not supported in the significand.

use std::char;
use std::str::{self,FromStr};

use nom_crate::*;

use expr::EvalResult;

#[derive(Debug,Copy,Clone,PartialEq,Eq)]
/// Representation of a C character
pub enum CChar {
	/// A character that can be represented as a `char`
	Char(char),
	/// Any other character (8-bit characters, unicode surrogates, etc.)
	Raw(u64),
}

impl From<u8> for CChar {
	fn from(i: u8) -> CChar {
		match i {
			0 ... 0x7f => CChar::Char(i as u8 as char),
			_ => CChar::Raw(i as u64),
		}
	}
}

// A non-allocating version of this would be nice...
impl Into<Vec<u8>> for CChar {
	fn into(self) -> Vec<u8> {
		match self {
			CChar::Char(c) => {
				let mut s=String::with_capacity(4);
				s.extend(&[c]);
				s.into_bytes()
			}
			CChar::Raw(i) => {
				let mut v=Vec::with_capacity(1);
				v.push(i as u8);
				v
			}
		}
	}
}

const OCTAL: &'static [u8]=b"01234567";
const DECIMAL: &'static [u8]=b"0123456789";
const HEX: &'static [u8]=b"0123456789abcdefABCDEF";

fn escape2char(c: char) -> CChar {
	CChar::Char(match c {
		'a' => '\x07',
		'b' => '\x08',
		'f' => '\x0c',
		'n' => '\n',
		'r' => '\r',
		't' => '\t',
		'v' => '\x0b',
		_ => unreachable!("invalid escape {}",c)
	})
}

fn c_raw_escape(n: &[u8], radix: u32) -> Option<CChar> {
	str::from_utf8(n).ok()
		.and_then(|i|u64::from_str_radix(i,radix).ok())
		.map(|i|match i {
			0 ... 0x7f => CChar::Char(i as u8 as char),
			_ => CChar::Raw(i),
		})
}

fn c_unicode_escape(n: Vec<char>) -> Option<CChar> {
	u32::from_str_radix(String::as_str(&n.into_iter().collect()),16).ok().and_then(char::from_u32).map(CChar::Char)
}

named!(escaped_char<CChar>,
	preceded!(char!('\\'),alt!(
		map!(one_of!(br#"'"?\"#),CChar::Char) |
		map!(one_of!(b"abfnrtv"),escape2char) |
		map_opt!(re_bytes_find_static!(r"^[0-7]{1,3}"),|v|c_raw_escape(v,8)) |
		map_opt!(preceded!(char!('x'),is_a!(HEX)),|v|c_raw_escape(v,16)) |
		map_opt!(preceded!(char!('u'),many_m_n!(4,4,one_of!(HEX))),c_unicode_escape) |
		map_opt!(preceded!(char!('U'),many_m_n!(8,8,one_of!(HEX))),c_unicode_escape)
	))
);

named!(c_width_prefix,
	alt!(
		tag!("u8") |
		tag!("u") |
		tag!("U") |
		tag!("L")
	)
);

named!(c_char<CChar>,
	delimited!(
		terminated!(opt!(c_width_prefix),char!('\'')),
		alt!( escaped_char | map!(le_u8,CChar::from) ),
		char!('\'')
	)
);

fn empty_vec(input: &[u8]) -> IResult<&[u8],Vec<u8>> {
	IResult::Done(input,vec![])
}

named!(c_string<Vec<u8> >,
	delimited!(
		alt!( preceded!(c_width_prefix,char!('"')) | char!('"') ),
		chain!(
			mut vec: empty_vec ~
			many0!(alt!(
				map!(tap!(c: escaped_char => { let v: Vec<u8>=c.into(); vec.extend_from_slice(&v) } ),|_|()) |
				map!(tap!(s: is_not!(b"\"") => vec.extend_from_slice(s) ),|_|())
			)),
			||{return vec}
		),
		char!('"')
	)
);

named!(c_int<i64>,
	terminated!(alt_complete!(
		map!(preceded!(tag!("0x"),is_a!(HEX)),
			|v|str::from_utf8(v).ok().and_then(|i|u64::from_str_radix(i,16).ok().map(|i|i as i64)).unwrap()) |
		map!(preceded!(tag!("0b"),is_a!(b"01")),
			|v|str::from_utf8(v).ok().and_then(|i|i64::from_str_radix(i,2).ok()).unwrap()) |
		map!(preceded!(char!('0'),is_a!(OCTAL)),
			|v|str::from_utf8(v).ok().and_then(|i|i64::from_str_radix(i,8).ok()).unwrap_or(0/*empty match*/)) |
		map!(is_a!(DECIMAL),
			|v|str::from_utf8(v).ok().and_then(|i|i64::from_str_radix(i,10).ok()).unwrap())
	),is_a!("ulUL"))
);

named!(c_float<f64>,
	map_opt!(terminated!(re_bytes_find_static!(r"^(\d*\.\d+|\d+\.?)(e[+-]?\d+)?"),opt!(complete!(one_of!("flFL")))),
		|v|str::from_utf8(v).ok().and_then(|i|f64::from_str(i).ok()))
);

named!(one_literal<&[u8],EvalResult,::Error>,
	fix_error!(::Error,alt_complete!(
		map!(c_char,EvalResult::Char) |
		map!(c_int,EvalResult::Int) |
		map!(c_float,EvalResult::Float) |
		map!(c_string,EvalResult::Str)
	))
);

/// Parse a C literal.
///
/// The input must contain exactly the representation of a single literal
/// token, and in particular no whitespace or sign prefixes.
pub fn parse(input: &[u8]) -> IResult<&[u8],EvalResult,::Error> {
	::assert_full_parse(one_literal(input))
}
