use alloc::{borrow::ToOwned, format, string::{String, ToString}, vec::Vec};
use core::cmp::Ordering;
use wasm_pdk::{Error, FromValue, Handle, Result, Value, decode, encode};

/// Full Python `json.dumps` kwargs supported.
pub struct Options {
    pub indent: Option<String>,
    pub sort_keys: bool,
    pub ensure_ascii: bool,
    pub check_circular: bool,
    pub allow_nan: bool,
    pub skipkeys: bool,
    pub item_sep: String,
    pub key_sep: String,
    pub cls: Option<Handle>,
    pub default: Option<Handle>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            indent: None, sort_keys: false, ensure_ascii: true,
            check_circular: true, allow_nan: true, skipkeys: false,
            item_sep: ", ".to_owned(), key_sep: ": ".to_owned(),
            cls: None, default: None,
        }
    }
}

pub struct SerCtx<'a> {
    pub opts: &'a Options,
}

/* Recursion-depth cap for circular detection, since the wasm plugin can't see Val identity (Python's `id(x) in seen`). Trips before JS call stack overflow on self-reference. */
const MAX_DEPTH: usize = 200;

pub fn serialize(value: &Handle, opts: Options) -> Result<String> {
    // `cls` short-circuits the whole walk, so instantiate and delegate to its `.encode(value)`.
    if let Some(cls) = &opts.cls {
        let encoder = cls.call("__call__", &[])?;
        let result = encoder.call("encode", &[value.raw()])?;
        return match decode(result.raw())? {
            Value::Bytes(b) => String::from_utf8(b).map_err(|e| Error::Value(format!("cls.encode produced non-UTF-8: {}", e))),
            _ => Err(Error::Type("cls.encode must return str".into())),
        };
    }
    let mut out = String::new();
    let mut ctx = SerCtx { opts: &opts };
    serialize_into(value, &mut out, &mut ctx, 0)?;
    Ok(out)
}

// Dispatch by `type_of`, sequences use `iter`+`len`+`get_item` and skip `iter_next` since wasm-pdk v0.1.0 StopIteration is broken.
fn serialize_into(value: &Handle, out: &mut String, ctx: &mut SerCtx, depth: usize) -> Result<()> {
    let ty_handle = value.type_of()?;
    let ty = String::from_handle(ty_handle.raw())?;
    match ty.as_str() {
        "NoneType" => { out.push_str("null"); Ok(()) }
        "bool" => {
            match decode(value.raw())? {
                Value::Bool(true) => out.push_str("true"),
                Value::Bool(false) => out.push_str("false"),
                _ => return Err(Error::Type("bool decoded as non-bool".into())),
            }
            Ok(())
        }
        "int" => {
            match decode(value.raw())? {
                Value::Int(i) => out.push_str(&format!("{}", i)),
                _ => return Err(Error::Type("int decoded as non-int".into())),
            }
            Ok(())
        }
        "float" => {
            out.push_str(&float_text(value, ctx.opts.allow_nan)?);
            Ok(())
        }
        "str" => {
            escape_string(&string(value)?, out, ctx.opts.ensure_ascii);
            Ok(())
        }
        "list" | "tuple" => serialize_sequence(value, out, ctx, depth),
        "dict" => serialize_object(value, out, ctx, depth),
        other => {
            if let Some(default) = &ctx.opts.default {
                let replacement = default.call("__call__", &[value.raw()])?;
                return serialize_into(&replacement, out, ctx, depth);
            }
            Err(Error::Type(format!("Object of type {} is not JSON serializable", other)))
        }
    }
}

fn serialize_sequence(value: &Handle, out: &mut String, ctx: &mut SerCtx, depth: usize) -> Result<()> {
    if ctx.opts.check_circular && depth >= MAX_DEPTH {
        return Err(Error::Value("Circular reference detected".into()));
    }
    out.push('[');
    let it = value.iter()?;
    let n = it.len()?;
    if n == 0 {
        out.push(']');
        return Ok(());
    }
    let opts = ctx.opts;
    for i in 0..n {
        let item = it.get_item(&index(i)?)?;
        if i > 0 { out.push_str(&opts.item_sep); }
        write_indent(out, opts.indent.as_deref(), depth + 1);
        serialize_into(&item, out, ctx, depth + 1)?;
    }
    write_indent(out, opts.indent.as_deref(), depth);
    out.push(']');
    Ok(())
}

fn serialize_object(value: &Handle, out: &mut String, ctx: &mut SerCtx, depth: usize) -> Result<()> {
    if ctx.opts.check_circular && depth >= MAX_DEPTH {
        return Err(Error::Value("Circular reference detected".into()));
    }
    out.push('{');
    let opts = ctx.opts;
    let view = value.call("items", &[])?.iter()?;
    let mut pairs = Vec::new();
    for i in 0..view.len()? {
        let pair = view.get_item(&index(i)?)?;
        pairs.push((pair.get_item(&index(0)?)?, pair.get_item(&index(1)?)?));
    }
    if opts.sort_keys {
        sort_keys(&mut pairs)?;
    }
    let mut written = 0;
    for (key, item) in &pairs {
        let Some(text) = key_text(key, opts)? else { continue };
        if written > 0 { out.push_str(&opts.item_sep); }
        written += 1;
        write_indent(out, opts.indent.as_deref(), depth + 1);
        escape_string(&text, out, opts.ensure_ascii);
        out.push_str(&opts.key_sep);
        serialize_into(item, out, ctx, depth + 1)?;
    }
    if written > 0 { write_indent(out, opts.indent.as_deref(), depth); }
    out.push('}');
    Ok(())
}

/* A key as sort_keys ranks it, numbers by value and strings by text. */
enum Rank { Int(i128), Float(f64), Text(String), Other }

impl Rank {
    fn of(key: &Handle) -> Rank {
        match decode(key.raw()) {
            Ok(Value::Int(n)) => Rank::Int(n),
            Ok(Value::Bool(b)) => Rank::Int(b as i128),
            Ok(Value::Float(f)) => Rank::Float(f),
            Ok(Value::Bytes(s)) => Rank::Text(String::from_utf8(s).unwrap_or_default()),
            _ => Rank::Other,
        }
    }

    fn kind(&self) -> u8 {
        match self { Rank::Int(_) | Rank::Float(_) => 0, Rank::Text(_) => 1, Rank::Other => 2 }
    }
}

// The engine cannot run list.sort for a plugin, so keys rank here as Python ranks them and refuse to as it refuses.
fn sort_keys(pairs: &mut Vec<(Handle, Handle)>) -> Result<()> {
    let mut ranked: Vec<(Rank, (Handle, Handle))> = pairs.drain(..).map(|pair| (Rank::of(&pair.0), pair)).collect();
    if let Some((_, (odd, _))) = ranked.iter().find(|(rank, _)| rank.kind() != ranked[0].0.kind()) {
        let name = |key: &Handle| key.type_of().and_then(|t| String::from_handle(t.raw()));
        return Err(Error::Type(format!("'<' not supported between instances of '{}' and '{}'", name(odd)?, name(&ranked[0].1.0)?)));
    }
    ranked.sort_by(|(a, _), (b, _)| match (a, b) {
        (Rank::Int(x), Rank::Int(y)) => x.cmp(y),
        (Rank::Int(x), Rank::Float(y)) => (*x as f64).partial_cmp(y).unwrap_or(Ordering::Equal),
        (Rank::Float(x), Rank::Int(y)) => x.partial_cmp(&(*y as f64)).unwrap_or(Ordering::Equal),
        (Rank::Float(x), Rank::Float(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        (Rank::Text(x), Rank::Text(y)) => x.cmp(y),
        _ => Ordering::Equal,
    });
    pairs.extend(ranked.into_iter().map(|(_, pair)| pair));
    Ok(())
}

// Python writes a number, a bool or None as the string it reads back as, skips another key under skipkeys and refuses it otherwise.
fn key_text(key: &Handle, opts: &Options) -> Result<Option<String>> {
    let ty = String::from_handle(key.type_of()?.raw())?;
    Ok(Some(match (ty.as_str(), decode(key.raw())) {
        ("str", _) => string(key)?,
        ("bool", Ok(Value::Bool(b))) => (if b { "true" } else { "false" }).to_owned(),
        ("int", Ok(Value::Int(i))) => i.to_string(),
        ("float", _) => float_text(key, opts.allow_nan)?,
        ("NoneType", _) => "null".to_owned(),
        _ if opts.skipkeys => return Ok(None),
        _ => return Err(Error::Type(format!("keys must be str, int, float, bool or None, not {ty}"))),
    }))
}

fn index(i: i64) -> Result<Handle> {
    encode(Value::Int(i as i128))
}

fn string(value: &Handle) -> Result<String> {
    match decode(value.raw())? {
        Value::Bytes(b) => String::from_utf8(b).map_err(|e| Error::Value(format!("invalid utf-8 in str: {}", e))),
        _ => Err(Error::Type("str decoded as non-bytes".into())),
    }
}

fn write_indent(out: &mut String, unit: Option<&str>, depth: usize) {
    if let Some(u) = unit {
        out.push('\n');
        for _ in 0..depth { out.push_str(u); }
    }
}

fn escape_string(s: &str, out: &mut String, ensure_ascii: bool) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0C}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c if ensure_ascii && (c as u32) >= 0x80 => {
                let cp = c as u32;
                if cp <= 0xFFFF {
                    out.push_str(&format!("\\u{:04x}", cp));
                } else {
                    // UTF-16 surrogate pair for code points beyond the BMP.
                    let v = cp - 0x10000;
                    let hi = 0xD800 + (v >> 10);
                    let lo = 0xDC00 + (v & 0x3FF);
                    out.push_str(&format!("\\u{:04x}\\u{:04x}", hi, lo));
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

// The engine writes the float, so dumps agrees with print, and only NaN and the infinities take JSON's names.
fn float_text(value: &Handle, allow_nan: bool) -> Result<String> {
    let Value::Float(f) = decode(value.raw())? else { return Err(Error::Type("float decoded as non-float".into())) };
    let text = String::from_handle(encode(Value::Bytes(b"{}".to_vec()))?.call("format", &[value.raw()])?.raw())?;
    if f.is_finite() {
        return Ok(text);
    }
    if !allow_nan {
        return Err(Error::Value(format!("Out of range float values are not JSON compliant: {text}")));
    }
    Ok((if f.is_nan() { "NaN" } else if f > 0.0 { "Infinity" } else { "-Infinity" }).to_owned())
}
