use alloc::{format, vec::Vec};
use wasm_pdk::{encode, Error, Handle, Result, Value};

use super::tokenizer::{JsonError, Token, Tokenizer};

/// Carries Python `json.loads` hooks, each replaces the default at its production.
pub struct LoadCtx {
    pub object_hook: Option<Handle>,
    pub object_pairs_hook: Option<Handle>,
    pub parse_float: Option<Handle>,
    pub parse_int: Option<Handle>,
    pub parse_constant: Option<Handle>,
}

const MAX_DEPTH: usize = 1000;

// Recursive-descent parser that reads delimiters the way Python's scanner does, before tokenizing what follows.
pub fn parse(src: &str, ctx: &LoadCtx) -> Result<Handle> {
    let mut tk = Tokenizer::new(src);
    if src.starts_with('\u{feff}') {
        return Err(to_pdk_err(tk.fail("Unexpected UTF-8 BOM (decode using utf-8-sig)", 0)));
    }
    let value = parse_value(&mut tk, ctx, 0)?;
    if tk.peek().is_some() {
        return Err(to_pdk_err(tk.fail("Extra data", tk.pos())));
    }
    Ok(value)
}

fn parse_value(tk: &mut Tokenizer, ctx: &LoadCtx, depth: usize) -> Result<Handle> {
    let t = tk.value().map_err(to_pdk_err)?;
    match t {
        Token::Null => encode(Value::None),
        Token::True => encode(Value::Bool(true)),
        Token::False => encode(Value::Bool(false)),
        Token::Int(i, src) => match &ctx.parse_int {
            Some(hook) => hook.call("__call__", &[encode(Value::Bytes(src.into_bytes()))?.raw()]),
            None => encode(Value::Int(i)),
        },
        Token::Float(f, src) => match &ctx.parse_float {
            Some(hook) => hook.call("__call__", &[encode(Value::Bytes(src.into_bytes()))?.raw()]),
            None => encode(Value::Float(f)),
        },
        Token::Constant(name) => match &ctx.parse_constant {
            Some(hook) => hook.call("__call__", &[encode(Value::Bytes(name.into_bytes()))?.raw()]),
            None => encode(Value::Float(match name.as_str() {
                "NaN" => f64::NAN,
                "Infinity" => f64::INFINITY,
                _ => f64::NEG_INFINITY,
            })),
        },
        Token::Str(s) => encode(Value::Bytes(s.into_bytes())),
        Token::LBracket | Token::LBrace if depth >= MAX_DEPTH => {
            Err(Error::Runtime(format!("maximum JSON nesting depth ({}) exceeded at byte {}", MAX_DEPTH, tk.pos())))
        }
        Token::LBracket => parse_array(tk, ctx, depth + 1),
        Token::LBrace => parse_object(tk, ctx, depth + 1),
    }
}

fn parse_array(tk: &mut Tokenizer, ctx: &LoadCtx, depth: usize) -> Result<Handle> {
    let list = Handle::new_list()?;
    if tk.peek() == Some(b']') {
        tk.bump();
        return Ok(list);
    }
    loop {
        let item = parse_value(tk, ctx, depth)?;
        let _ = list.call("append", &[item.raw()])?;
        match tk.peek() {
            Some(b']') => { tk.bump(); return Ok(list) }
            Some(b',') => tk.bump(),
            _ => return Err(to_pdk_err(tk.fail("Expecting ',' delimiter", tk.pos()))),
        }
    }
}

fn parse_object(tk: &mut Tokenizer, ctx: &LoadCtx, depth: usize) -> Result<Handle> {
    let mut pairs: Vec<(Handle, Handle)> = Vec::new();
    match tk.peek() {
        Some(b'}') => tk.bump(),
        Some(b'"') => loop {
            let Token::Str(name) = tk.value().map_err(to_pdk_err)? else { unreachable!("a key opens with a quote") };
            let key = encode(Value::Bytes(name.into_bytes()))?;
            if tk.peek() != Some(b':') {
                return Err(to_pdk_err(tk.fail("Expecting ':' delimiter", tk.pos())));
            }
            tk.bump();
            pairs.push((key, parse_value(tk, ctx, depth)?));
            match tk.peek() {
                Some(b'}') => { tk.bump(); break }
                Some(b',') => tk.bump(),
                _ => return Err(to_pdk_err(tk.fail("Expecting ',' delimiter", tk.pos()))),
            }
            if tk.peek() != Some(b'"') {
                return Err(to_pdk_err(tk.fail("Expecting property name enclosed in double quotes", tk.pos())));
            }
        },
        _ => return Err(to_pdk_err(tk.fail("Expecting property name enclosed in double quotes", tk.pos()))),
    }
    // `object_pairs_hook` wins over `object_hook` per Python spec, and takes the pairs as a list of tuples.
    if let Some(hook) = &ctx.object_pairs_hook {
        let list = Handle::new_list()?;
        for (k, v) in &pairs {
            list.call("append", &[Handle::new_tuple(&[k.raw(), v.raw()])?.raw()])?;
        }
        return hook.call("__call__", &[list.raw()]);
    }
    let dict = Handle::new_dict()?;
    for (k, v) in &pairs {
        dict.set_item(k, v)?;
    }
    match &ctx.object_hook {
        Some(hook) => hook.call("__call__", &[dict.raw()]),
        None => Ok(dict),
    }
}

fn to_pdk_err(e: JsonError) -> Error {
    Error::Value(e.0)
}
