#![no_std]
#![cfg_attr(target_arch = "wasm32", no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use wasm_pdk::*;

module_fixed_pool!();

pub mod parser;
pub mod serializer;
pub mod tokenizer;

#[plugin_fn]
fn loads(document: Handle, kw: Kwargs) -> Result<Handle> {
    // A str reads as it is and bytes decode through the engine, past the byte order mark json.loads drops.
    let text = match decode(document.raw()) {
        Ok(Value::Bytes(text)) => String::from_utf8(text).map_err(|e| Error::Value(e.to_string()))?,
        Ok(Value::Raw(_)) => {
            let text = String::from_handle(document.call("decode", &[encode(Value::Bytes(b"utf-8".to_vec()))?.raw()])?.raw())?;
            text.strip_prefix('\u{feff}').map(String::from).unwrap_or(text)
        }
        _ => return Err(Error::Type(format!("the JSON object must be str, bytes or bytearray, not {}", type_name(&document)?))),
    };
    let ctx = parser::LoadCtx {
        object_hook: kw.get_handle("object_hook")?,
        object_pairs_hook: kw.get_handle("object_pairs_hook")?,
        parse_float: kw.get_handle("parse_float")?,
        parse_int: kw.get_handle("parse_int")?,
        parse_constant: kw.get_handle("parse_constant")?,
    };
    parser::parse(&text, &ctx)
}

#[plugin_fn]
fn dumps(value: Handle, kw: Kwargs) -> Result<String> {
    let indent = indent_of(kw.get_handle("indent")?)?;
    let mut opts = serializer::Options {
        sort_keys: kw.get::<bool>("sort_keys")?.unwrap_or(false),
        ensure_ascii: kw.get::<bool>("ensure_ascii")?.unwrap_or(true),
        check_circular: kw.get::<bool>("check_circular")?.unwrap_or(true),
        allow_nan: kw.get::<bool>("allow_nan")?.unwrap_or(true),
        skipkeys: kw.get::<bool>("skipkeys")?.unwrap_or(false),
        cls: kw.get_handle("cls")?,
        default: kw.get_handle("default")?,
        // Python spaces both separators, except the comma an indent already follows with a newline.
        item_sep: (if indent.is_some() { "," } else { ", " }).to_string(),
        key_sep: ": ".to_string(),
        indent,
    };
    // `separators` arrives as a 2-tuple `(item, key)`, indexed manually since `Kwargs::get` can't decode tuples.
    if let Some(seps) = kw.get_handle("separators")? {
        let zero = encode(Value::Int(0))?;
        let one = encode(Value::Int(1))?;
        let item = seps.get_item(&zero)?;
        let key = seps.get_item(&one)?;
        opts.item_sep = decode_str(&item, "separators[0]")?;
        opts.key_sep = decode_str(&key, "separators[1]")?;
    }
    serializer::serialize(&value, opts)
}

// An int indents by that many spaces and a str by itself, as Python reads `indent`.
fn indent_of(indent: Option<Handle>) -> Result<Option<String>> {
    let Some(indent) = indent else { return Ok(None) };
    Ok(Some(match decode(indent.raw())? {
        Value::Bytes(unit) => String::from_utf8(unit).unwrap_or_default(),
        Value::Int(n) => " ".repeat(n.max(0) as usize),
        Value::Bool(b) => " ".repeat(b as usize),
        _ => return Err(Error::Type(format!("can't multiply sequence by non-int of type '{}'", type_name(&indent)?))),
    }))
}

fn type_name(value: &Handle) -> Result<String> {
    String::from_handle(value.type_of()?.raw())
}

fn decode_str(h: &Handle, what: &str) -> Result<String> {
    match decode(h.raw())? {
        Value::Bytes(b) => String::from_utf8(b).map_err(|e| Error::Value(format!("{} not UTF-8: {}", what, e))),
        _ => Err(Error::Type(format!("{} must be str", what))),
    }
}
