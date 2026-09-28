use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

/* Why a format or a value is refused, the variant decides the host exception kind. */
#[derive(Debug, PartialEq)]
pub enum PackError {
    Struct(String), // struct.error
    Overflow(String), // a float too large for its format
}

fn refuse<T>(message: impl Into<String>) -> Result<T, PackError> {
    Err(PackError::Struct(message.into()))
}

/* One value a format holds, what crosses between the plugin and the program. */
#[derive(Clone, Debug, PartialEq)]
pub enum Field {
    None,
    Bool(bool),
    Int(i128),
    Float(f64),
    Str(String),
    Bytes(Vec<u8>),
    // A list or a dict, which only `?` takes, by whether it is empty.
    Other(bool),
}

/* One code of a format and where its first value sits. */
struct Item {
    code: u8,
    count: usize,
    width: usize,
    offset: usize,
}

/* A parsed format, its byte order, its items and the size they fill. */
pub struct Layout {
    big: bool,
    items: Vec<Item>,
    pub size: usize,
}

// Bytes a code takes in standard sizes, None for a code only the native mode knows.
fn standard(code: u8) -> Option<usize> {
    Some(match code {
        b'x' | b'c' | b'b' | b'B' | b'?' | b's' | b'p' => 1,
        b'h' | b'H' | b'e' => 2,
        b'i' | b'I' | b'l' | b'L' | b'f' => 4,
        b'q' | b'Q' | b'd' => 8,
        _ => return None,
    })
}

// Bytes a code takes natively, the sizes of a 64-bit platform.
fn native(code: u8) -> Option<usize> {
    match code {
        b'l' | b'L' | b'n' | b'N' | b'P' => Some(8),
        _ => standard(code),
    }
}

/* Reads a format as Python does, `@` sizes and aligns natively unless another order is named. */
pub fn parse(format: &str) -> Result<Layout, PackError> {
    let bytes = format.as_bytes();
    let (big, is_native, mut i) = match bytes.first() {
        Some(b'@') => (false, true, 1),
        Some(b'=') | Some(b'<') => (false, false, 1),
        Some(b'>') | Some(b'!') => (true, false, 1),
        _ => (false, true, 0),
    };
    let mut items = Vec::new();
    let mut size: usize = 0;
    while i < bytes.len() {
        // Whitespace may separate items, never a count from its code.
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        let mut count: usize = 0;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            count = count.checked_mul(10).and_then(|n| n.checked_add((bytes[i] - b'0') as usize)).ok_or_else(|| PackError::Struct(String::from("total struct size too long")))?;
            i += 1;
        }
        let Some(&code) = bytes.get(i) else { return refuse("repeat count given without format specifier") };
        i += 1;
        let count = if i - 1 > start { count } else { 1 };
        let width = if is_native { native(code) } else { standard(code) }.ok_or_else(|| PackError::Struct(String::from("bad char in struct format")))?;
        // Native items sit at a multiple of their own width, a string or a pad byte anywhere.
        if is_native && !matches!(code, b's' | b'p' | b'x') {
            size = size.div_ceil(width) * width;
        }
        items.push(Item { code, count, width, offset: size });
        let span = if matches!(code, b's' | b'p') { count } else { count.checked_mul(width).ok_or_else(|| PackError::Struct(String::from("total struct size too long")))? };
        size = size.checked_add(span).ok_or_else(|| PackError::Struct(String::from("total struct size too long")))?;
    }
    Ok(Layout { big, items, size })
}

impl Layout {
    /* How many values the format packs, a string is one value and a pad byte none. */
    pub fn values(&self) -> usize {
        self.items.iter().map(|it| match it.code {
            b'x' => 0,
            b's' | b'p' => 1,
            _ => it.count,
        }).sum()
    }

    pub fn pack(&self, values: &[Field]) -> Result<Vec<u8>, PackError> {
        if values.len() != self.values() {
            return refuse(format!("pack expected {} items for packing (got {})", self.values(), values.len()));
        }
        let mut out = vec![0u8; self.size];
        let mut next = values.iter();
        for it in &self.items {
            match it.code {
                b'x' => {}
                b's' | b'p' => {
                    let Some(Field::Bytes(data)) = next.next() else { return refuse(format!("argument for '{}' must be a bytes object", it.code as char)) };
                    let field = &mut out[it.offset..it.offset + it.count];
                    if it.code == b's' {
                        let n = data.len().min(it.count);
                        field[..n].copy_from_slice(&data[..n]);
                    } else if it.count > 0 {
                        let n = data.len().min(it.count - 1).min(255);
                        field[0] = n as u8;
                        field[1..1 + n].copy_from_slice(&data[..n]);
                    }
                }
                _ => {
                    for k in 0..it.count {
                        let at = it.offset + k * it.width;
                        let value = next.next().unwrap_or(&Field::None);
                        put(&mut out[at..at + it.width], self.big, it.code, value)?;
                    }
                }
            }
        }
        Ok(out)
    }

    pub fn unpack(&self, data: &[u8]) -> Result<Vec<Field>, PackError> {
        if data.len() != self.size {
            return refuse(format!("unpack requires a buffer of {} bytes", self.size));
        }
        let mut out = Vec::with_capacity(self.values());
        for it in &self.items {
            let field = &data[it.offset..];
            match it.code {
                b'x' => {}
                b's' => out.push(Field::Bytes(field[..it.count].to_vec())),
                b'p' if it.count == 0 => out.push(Field::Bytes(Vec::new())),
                b'p' => {
                    let n = (field[0] as usize).min(it.count - 1);
                    out.push(Field::Bytes(field[1..1 + n].to_vec()));
                }
                _ => {
                    for k in 0..it.count {
                        out.push(take(&field[k * it.width..(k + 1) * it.width], self.big, it.code));
                    }
                }
            }
        }
        Ok(out)
    }
}

// The range an integer code holds, by its width and whether it is signed.
fn range(code: u8, width: usize) -> (i128, i128) {
    let bits = 8 * width as u32;
    match code {
        b'b' | b'h' | b'i' | b'l' | b'q' | b'n' => (-(1i128 << (bits - 1)), (1i128 << (bits - 1)) - 1),
        // A pointer takes either reading of its bits.
        b'P' => (-(1i128 << (bits - 1)), (1i128 << bits) - 1),
        _ => (0, (1i128 << bits) - 1),
    }
}

fn put(slot: &mut [u8], big: bool, code: u8, value: &Field) -> Result<(), PackError> {
    match code {
        b'?' => slot[0] = truthy(value) as u8,
        b'c' => match value {
            Field::Bytes(b) if b.len() == 1 => slot[0] = b[0],
            _ => return refuse("char format requires a bytes object of length 1"),
        },
        b'e' | b'f' | b'd' => {
            let x = match value {
                Field::Float(f) => *f,
                Field::Int(i) => *i as f64,
                Field::Bool(b) => *b as u8 as f64,
                _ => return refuse("required argument is not a float"),
            };
            let bits: u64 = match code {
                b'e' => half(x)? as u64,
                b'f' => {
                    let y = x as f32;
                    if y.is_infinite() && !x.is_infinite() {
                        return Err(PackError::Overflow(String::from("float too large to pack with f format")));
                    }
                    y.to_bits() as u64
                }
                _ => x.to_bits(),
            };
            write(slot, big, bits as u128);
        }
        _ => {
            let i = match value {
                Field::Int(i) => *i,
                Field::Bool(b) => *b as i128,
                _ => return refuse("required argument is not an integer"),
            };
            let (min, max) = range(code, slot.len());
            if !(min..=max).contains(&i) {
                return refuse(format!("'{}' format requires {} <= number <= {}", code as char, min, max));
            }
            write(slot, big, i as u128);
        }
    }
    Ok(())
}

fn take(slot: &[u8], big: bool, code: u8) -> Field {
    let bits = read(slot, big);
    match code {
        b'?' => Field::Bool(bits != 0),
        b'c' => Field::Bytes(slot.to_vec()),
        b'e' => Field::Float(unhalf(bits as u16)),
        b'f' => Field::Float(f32::from_bits(bits as u32) as f64),
        b'd' => Field::Float(f64::from_bits(bits as u64)),
        b'b' | b'h' | b'i' | b'l' | b'q' | b'n' => {
            // Sign extension from the width the code took.
            let shift = 128 - 8 * slot.len() as u32;
            Field::Int(((bits << shift) as i128) >> shift)
        }
        _ => Field::Int(bits as i128),
    }
}

// The low bytes of `bits`, in the byte order the format names.
fn write(slot: &mut [u8], big: bool, bits: u128) {
    let le = bits.to_le_bytes();
    for (k, byte) in slot.iter_mut().enumerate() {
        *byte = le[k];
    }
    if big {
        slot.reverse();
    }
}

fn read(slot: &[u8], big: bool) -> u128 {
    let mut bits = 0u128;
    for k in 0..slot.len() {
        let byte = if big { slot[k] } else { slot[slot.len() - 1 - k] };
        bits = (bits << 8) | byte as u128;
    }
    bits
}

fn truthy(value: &Field) -> bool {
    match value {
        Field::None => false,
        Field::Bool(b) => *b,
        Field::Int(i) => *i != 0,
        Field::Float(f) => *f != 0.0,
        Field::Str(s) => !s.is_empty(),
        Field::Bytes(b) => !b.is_empty(),
        Field::Other(full) => *full,
    }
}

/* A double as IEEE half precision, rounding half to even the way Python does. */
fn half(x: f64) -> Result<u16, PackError> {
    let overflow = || PackError::Overflow(String::from("float too large to pack with e format"));
    let sign = (x.is_sign_negative() as u16) << 15;
    if x == 0.0 {
        return Ok(sign);
    }
    if x.is_infinite() {
        return Ok(sign | 0x7c00);
    }
    if x.is_nan() {
        return Ok(sign | 0x7e00);
    }
    let (f, e) = libm::frexp(x.abs());
    // Normalized to a fraction in [1, 2) and its exponent.
    let (mut f, mut e) = (f * 2.0, e - 1);
    if e >= 16 {
        return Err(overflow());
    } else if e < -25 {
        f = 0.0;
        e = 0;
    } else if e < -14 {
        f = libm::ldexp(f, 14 + e);
        e = 0;
    } else {
        e += 15;
        f -= 1.0;
    }
    f *= 1024.0;
    let mut bits = f as u16;
    let rest = f - bits as f64;
    if rest > 0.5 || (rest == 0.5 && bits % 2 == 1) {
        bits += 1;
        if bits == 1024 {
            bits = 0;
            e += 1;
            if e == 31 {
                return Err(overflow());
            }
        }
    }
    Ok(sign | ((e as u16) << 10) | bits)
}

fn unhalf(h: u16) -> f64 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let (e, m) = ((h >> 10) & 0x1f, (h & 0x3ff) as f64);
    match e {
        0 => sign * m * (1.0 / 16_777_216.0),
        31 if m == 0.0 => sign * f64::INFINITY,
        31 => f64::NAN,
        _ => sign * libm::ldexp(1.0 + m / 1024.0, e as i32 - 15),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Native items sit at a multiple of their width, the standard orders pack them tight.
    #[test]
    fn native_formats_align_and_standard_ones_do_not() {
        for (format, size) in [("bi", 8), ("ib", 5), ("ib0i", 8), ("bq", 16), ("?h", 4), ("c3sh", 6), ("xd", 16), ("<bi", 5), ("< i h", 6)] {
            assert_eq!(parse(format).unwrap().size, size, "{format}");
        }
    }

    #[test]
    fn half_floats_round_to_even_and_overflow_like_python() {
        assert_eq!(half(1.5).unwrap(), 0x3e00);
        assert_eq!(half(65504.0).unwrap(), 0x7bff);
        assert!(half(65520.0).is_err());
        assert_eq!(unhalf(0x3c00), 1.0);
    }
}
