#![no_std]
#![cfg_attr(target_arch = "wasm32", no_main)]

extern crate alloc;

wasm_pdk::module_fixed_pool!();

pub mod pack;

/* The plugin packs and unpacks, and main.py adds the tuples, Struct and the offsets. */
mod wasm_api {
    use alloc::string::String;
    use alloc::vec::Vec;
    use wasm_pdk::*;
    use crate::pack::{self, Field, PackError};

    // The kind that crosses as a named exception, OverflowError here.
    const CUSTOM: u32 = 6;

    fn to_error(e: PackError) -> Error {
        match e {
            PackError::Struct(m) => Error::Value(m), // struct.error is ValueError
            PackError::Overflow(m) => Error::Custom { kind: CUSTOM, message: alloc::format!("OverflowError: {m}") },
        }
    }

    fn field(value: Value) -> Field {
        match value {
            Value::None => Field::None,
            Value::Bool(b) => Field::Bool(b),
            Value::Int(i) => Field::Int(i),
            Value::Float(f) => Field::Float(f),
            Value::Bytes(s) => Field::Str(String::from_utf8(s).unwrap_or_default()),
            Value::Raw(b) => Field::Bytes(b),
            Value::List(l) => Field::Other(!l.is_empty()),
            Value::Dict(d) => Field::Other(!d.is_empty()),
        }
    }

    fn value(field: Field) -> Value {
        match field {
            Field::Bool(b) => Value::Bool(b),
            Field::Int(i) => Value::Int(i),
            Field::Float(f) => Value::Float(f),
            Field::Bytes(b) => Value::Raw(b),
            _ => Value::None,
        }
    }

    /* Packs the values into the bytes the format describes. */
    #[plugin_fn]
    fn pack(format: String, values: Args) -> Result<Bytes> {
        let layout = pack::parse(&format).map_err(to_error)?;
        let mut fields = Vec::with_capacity(values.len());
        for i in 0..values.len() {
            let v: Value = values.get(i).unwrap()?;
            fields.push(field(v));
        }
        layout.pack(&fields).map(Bytes).map_err(to_error)
    }

    /* The values a buffer of exactly the format's size holds, in order. */
    #[plugin_fn]
    fn unpack(format: String, buffer: Bytes) -> Result<Vec<Value>> {
        let layout = pack::parse(&format).map_err(to_error)?;
        Ok(layout.unpack(&buffer).map_err(to_error)?.into_iter().map(value).collect())
    }

    /* The bytes the format takes. */
    #[plugin_fn]
    fn calcsize(format: String) -> Result<i64> {
        Ok(pack::parse(&format).map_err(to_error)?.size as i64)
    }
}
