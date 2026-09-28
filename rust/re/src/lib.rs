#![no_std]
#![cfg_attr(target_arch = "wasm32", no_main)]

extern crate alloc;

wasm_pdk::module_fixed_pool!();

pub mod ast;
pub mod engine;
pub mod matcher;
pub mod parser;

/* The plugin hands back spans, and main.py builds the Match and Pattern objects over them. */
mod wasm_api {
    use alloc::string::String;
    use alloc::vec::Vec;
    use wasm_pdk::*;
    use crate::ast::Flags;
    use crate::engine::{self, Mode, ReError};
    use crate::matcher::Caps;

    // The kind that crosses as a named exception, OverflowError here.
    const CUSTOM: u32 = 6;

    /* Routes engine errors to the matching host exception kind. */
    fn to_error(e: ReError) -> Error {
        match e {
            ReError::Syntax(m) => Error::Value(m), // bad pattern is a ValueError
            ReError::TooComplex(m) => Error::Runtime(m), // degradation is a RuntimeError
            ReError::Overflow(m) => Error::Custom { kind: CUSTOM, message: alloc::format!("OverflowError: {m}") },
            ReError::Index(m) => Error::Index(m),
        }
    }

    fn rx<T>(r: core::result::Result<T, ReError>) -> Result<T> {
        r.map_err(to_error)
    }

    /* Compiled-pattern cache keyed by pattern and flags, every call path compiles a pair once. Capped so unbounded pattern churn stays bounded. */
    static PATTERNS: PluginCell<Vec<(String, i64, engine::Regex)>> = PluginCell::new();

    fn with_compiled<T>(pattern: &str, flags: i64, f: impl FnOnce(&engine::Regex) -> Result<T>) -> Result<T> {
        let cache = PATTERNS.get_or_init(Vec::new);
        let idx = match cache.iter().position(|(p, fl, _)| p == pattern && *fl == flags) {
            Some(i) => i,
            None => {
                let re = rx(engine::Regex::compile(pattern, Flags::from_bits(flags as u32)))?;
                if cache.len() >= 64 { cache.remove(0); }
                cache.push((String::from(pattern), flags, re));
                cache.len() - 1
            }
        };
        f(&cache[idx].2)
    }

    /* Every group's start and end in codepoints, -1 for a group that took no part. */
    fn spans(caps: &Caps) -> Vec<Value> {
        caps.iter().flat_map(|g| {
            let (s, e) = g.map_or((-1, -1), |(s, e)| (s as i128, e as i128));
            [Value::Int(s), Value::Int(e)]
        }).collect()
    }

    /* The group count, each named group's index and the global flags, what a compiled pattern reports. */
    #[plugin_fn]
    fn info(pattern: String, flags: i64) -> Result<Vec<Value>> {
        with_compiled(&pattern, flags, |re| {
            let names = re.names().iter().map(|(name, at)| (Value::Bytes(name.clone().into_bytes()), Value::Int(*at as i128))).collect();
            Ok(alloc::vec![Value::Int(re.group_count() as i128), Value::Dict(names), Value::Int(re.flags() as i128)])
        })
    }

    /* The first match as its spans, mode 0 searches, 1 anchors at the start, 2 takes it all. */
    #[plugin_fn]
    fn find(pattern: String, flags: i64, string: String, mode: i64) -> Result<Option<Vec<Value>>> {
        let mode = match mode { 1 => Mode::Match, 2 => Mode::Full, _ => Mode::Search };
        with_compiled(&pattern, flags, |re| Ok(rx(engine::find_rx(re, &string, mode))?.map(|caps| spans(&caps))))
    }

    /* Every match as its spans, at most `limit` unless it is zero, in one crossing. */
    #[plugin_fn]
    fn find_all(pattern: String, flags: i64, string: String, limit: i64) -> Result<Vec<Value>> {
        with_compiled(&pattern, flags, |re| Ok(rx(engine::find_all_rx(re, &string, limit.max(0) as usize))?.iter().map(|caps| Value::List(spans(caps))).collect()))
    }

    /* A template replacing up to `count` matches, the new text and how many it replaced. */
    #[plugin_fn]
    fn sub(pattern: String, flags: i64, repl: String, string: String, count: i64) -> Result<Vec<Value>> {
        with_compiled(&pattern, flags, |re| {
            let (text, n) = rx(engine::sub_rx(re, &repl, &string, count.max(0) as usize))?;
            Ok(alloc::vec![Value::Bytes(text.into_bytes()), Value::Int(n as i128)])
        })
    }
}
