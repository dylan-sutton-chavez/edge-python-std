#![no_std]
#![cfg_attr(target_arch = "wasm32", no_main)]

extern crate alloc;

wasm_pdk::module_fixed_pool!();

/* The plugin holds the functions, and main.py adds the constants and prod. */
pub mod float;
pub mod int;

use alloc::format;
use alloc::string::String;
use wasm_pdk::*;

// The kind that crosses as a named exception, OverflowError or ZeroDivisionError here.
const CUSTOM: u32 = 6;

fn raise(class: &str, message: &str) -> Error {
    Error::Custom { kind: CUSTOM, message: format!("{class}: {message}") }
}

fn domain() -> Error {
    Error::Value(String::from("math domain error"))
}

// Past 128 bits an int overflows with the words the engine's own arithmetic uses.
fn too_large() -> Error {
    raise("OverflowError", "integer too large for 128-bit int range")
}

fn type_name(h: u32) -> String {
    Handle::borrow(h).type_of().and_then(|t| String::from_handle(t.raw())).unwrap_or_default()
}

fn not_real(h: u32) -> Error {
    Error::Type(format!("must be real number, not {}", type_name(h)))
}

/* A float argument, an int or a bool converts to one as in Python. */
pub struct Real(pub f64);

impl FromValue for Real {
    fn from_handle(h: u32) -> Result<Self> {
        match decode(h) {
            Ok(Value::Float(f)) => Ok(Real(f)),
            Ok(Value::Int(i)) => Ok(Real(i as f64)),
            Ok(Value::Bool(b)) => Ok(Real(b as u8 as f64)),
            _ => Err(not_real(h)),
        }
    }
}

/* An integer argument, a bool counts and a float does not. */
pub struct Integer(pub i128);

impl FromValue for Integer {
    fn from_handle(h: u32) -> Result<Self> {
        match decode(h) {
            Ok(Value::Int(i)) => Ok(Integer(i)),
            Ok(Value::Bool(b)) => Ok(Integer(b as i128)),
            _ => Err(Error::Type(format!("'{}' object cannot be interpreted as an integer", type_name(h)))),
        }
    }
}
