use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use wasm_pdk::*;
use crate::{domain, not_real, raise, too_large, Integer, Real};

// A NaN out of a number leaves the domain, an infinity out of a finite one overflows or hits a pole.
fn checked(x: f64, r: f64, overflows: bool) -> Result<f64> {
    if r.is_nan() && !x.is_nan() {
        return Err(domain());
    }
    if r.is_infinite() && x.is_finite() {
        return Err(if overflows { raise("OverflowError", "math range error") } else { domain() });
    }
    Ok(r)
}

// Each wraps libm under that rule, and the flag says whether an infinity is an overflow.
macro_rules! unary { ($($name:ident $overflows:literal),* $(,)?) => { $(
    #[plugin_fn]
    fn $name(x: Real) -> Result<f64> { checked(x.0, libm::$name(x.0), $overflows) }
)* } }

unary!(
    acos false, acosh false, asin false, asinh false, atan false, atanh false, cbrt false,
    cos false, cosh true, erf false, erfc false, exp true, exp2 true, expm1 true, fabs false,
    log1p false, log2 false, log10 false, sin false, sinh true, sqrt false, tan false, tanh false,
);

// With two arguments only a NaN out of numbers is an error.
macro_rules! binary { ($($name:ident),* $(,)?) => { $(
    #[plugin_fn]
    fn $name(x: Real, y: Real) -> Result<f64> {
        let r = libm::$name(x.0, y.0);
        if r.is_nan() && !x.0.is_nan() && !y.0.is_nan() { return Err(domain()); }
        Ok(r)
    }
)* } }

binary!(atan2, copysign, fmod, remainder);

#[plugin_fn]
fn pow(x: Real, y: Real) -> Result<f64> {
    let (x, y) = (x.0, y.0);
    let r = libm::pow(x, y);
    // From finite operands NaN is a negative base to a fraction, and infinity a pole or an overflow.
    if x.is_finite() && y.is_finite() && !r.is_finite() {
        return Err(if r.is_infinite() && x != 0.0 { raise("OverflowError", "math range error") } else { domain() });
    }
    Ok(r)
}

/* The natural log, or with a base the ratio of the two logs. */
#[plugin_fn]
fn log(x: Real, base: Args) -> Result<f64> {
    let ln = |v: f64| checked(v, libm::log(v), false);
    let num = ln(x.0)?;
    match base.len() {
        0 => Ok(num),
        1 => {
            let den = ln(base.get::<Real>(0).unwrap()?.0)?;
            if den == 0.0 {
                return Err(raise("ZeroDivisionError", "float division by zero"));
            }
            Ok(num / den)
        }
        n => Err(Error::Type(format!("log expected at most 2 arguments, got {}", n + 1))),
    }
}

// Zero, a negative integer and minus infinity have no gamma, and a huge one overflows.
#[plugin_fn]
fn gamma(x: Real) -> Result<f64> {
    if x.0 <= 0.0 && x.0 == libm::floor(x.0) {
        return Err(domain());
    }
    checked(x.0, libm::tgamma(x.0), true)
}

#[plugin_fn]
fn lgamma(x: Real) -> Result<f64> {
    if x.0.is_finite() && x.0 <= 0.0 && x.0 == libm::floor(x.0) {
        return Err(domain());
    }
    checked(x.0, libm::lgamma(x.0), true)
}

#[plugin_fn]
fn ldexp(x: Real, i: Integer) -> Result<f64> {
    // An exponent past what i32 holds overflows or underflows all the same.
    let i = i.0.clamp(i32::MIN as i128, i32::MAX as i128) as i32;
    checked(x.0, libm::ldexp(x.0, i), true)
}

fn pair(a: Value, b: Value) -> Result<Handle> {
    Handle::new_tuple(&[encode(a)?.raw(), encode(b)?.raw()])
}

#[plugin_fn]
fn frexp(x: Real) -> Result<Handle> {
    let (mantissa, exponent) = libm::frexp(x.0);
    pair(Value::Float(mantissa), Value::Int(exponent as i128))
}

#[plugin_fn]
fn modf(x: Real) -> Result<Handle> {
    let (fraction, whole) = libm::modf(x.0);
    pair(Value::Float(fraction), Value::Float(whole))
}

// An int comes back as it is, and a float rounds to the int it names.
fn integral(x: Handle, round: fn(f64) -> f64) -> Result<i128> {
    let r = match decode(x.raw()) {
        Ok(Value::Int(i)) => return Ok(i),
        Ok(Value::Bool(b)) => return Ok(b as i128),
        Ok(Value::Float(f)) => round(f),
        _ => return Err(not_real(x.raw())),
    };
    if r.is_nan() {
        return Err(Error::Value(String::from("cannot convert float NaN to integer")));
    }
    if r.is_infinite() {
        return Err(raise("OverflowError", "cannot convert float infinity to integer"));
    }
    if r < i128::MIN as f64 || r >= -(i128::MIN as f64) {
        return Err(too_large());
    }
    Ok(r as i128)
}

#[plugin_fn]
fn floor(x: Handle) -> Result<i128> { integral(x, libm::floor) }

#[plugin_fn]
fn ceil(x: Handle) -> Result<i128> { integral(x, libm::ceil) }

#[plugin_fn]
fn trunc(x: Handle) -> Result<i128> { integral(x, libm::trunc) }

#[plugin_fn]
fn isfinite(x: Real) -> bool { x.0.is_finite() }

#[plugin_fn]
fn isinf(x: Real) -> bool { x.0.is_infinite() }

#[plugin_fn]
fn isnan(x: Real) -> bool { x.0.is_nan() }

#[plugin_fn]
fn degrees(x: Real) -> f64 { x.0 * (180.0 / core::f64::consts::PI) }

#[plugin_fn]
fn radians(x: Real) -> f64 { x.0 * (core::f64::consts::PI / 180.0) }

/* Whether a and b sit within a tolerance relative to the larger one, or an absolute one. */
#[plugin_fn]
fn isclose(a: Real, b: Real, tolerances: Kwargs) -> Result<bool> {
    let rel = tolerances.get::<Real>("rel_tol")?.map_or(1e-09, |t| t.0);
    let abs = tolerances.get::<Real>("abs_tol")?.map_or(0.0, |t| t.0);
    if rel < 0.0 || abs < 0.0 {
        return Err(Error::Value(String::from("tolerances must be non-negative")));
    }
    let (a, b) = (a.0, b.0);
    if a == b {
        return Ok(true);
    }
    if a.is_infinite() || b.is_infinite() {
        return Ok(false);
    }
    let diff = (b - a).abs();
    Ok(diff <= (rel * b).abs() || diff <= (rel * a).abs() || diff <= abs)
}

// Every value an iterable yields as a float, where a list or a tuple of numbers crosses whole.
fn reals(values: Handle) -> Result<Vec<f64>> {
    if let Ok(all) = Vec::<f64>::from_handle(values.raw()) {
        return Ok(all);
    }
    let items = values.iter()?;
    let mut out = Vec::new();
    while let Some(item) = items.iter_next()? {
        out.push(Real::from_handle(item.raw())?.0);
    }
    Ok(out)
}

#[plugin_fn]
fn hypot(coordinates: Args) -> Result<f64> {
    let values = coordinates.0.iter().map(|h| Real::from_handle(h.raw()).map(|r| r.0)).collect::<Result<Vec<_>>>()?;
    Ok(norm(values))
}

#[plugin_fn]
fn dist(p: Handle, q: Handle) -> Result<f64> {
    let (p, q) = (reals(p)?, reals(q)?);
    if p.len() != q.len() {
        return Err(Error::Value(String::from("both points must have the same number of dimensions")));
    }
    Ok(norm(p.iter().zip(&q).map(|(a, b)| a - b).collect()))
}

#[plugin_fn]
fn fsum(values: Handle) -> Result<f64> {
    exact_sum(&reals(values)?)
}

/* The Euclidean norm, an infinity winning over NaN as in Python. */
pub fn norm(mut values: Vec<f64>) -> f64 {
    values.iter_mut().for_each(|v| *v = v.abs());
    let max = values.iter().fold(0.0, |m: f64, &v| if v > m { v } else { m });
    if max.is_infinite() {
        return max;
    }
    if values.iter().any(|v| v.is_nan()) {
        return f64::NAN;
    }
    scaled(&mut values, max)
}

// Scaled below one and squared without loss, then corrected once, the way Python does it.
fn scaled(values: &mut [f64], max: f64) -> f64 {
    if max == 0.0 || values.len() <= 1 {
        return max;
    }
    let (_, e) = libm::frexp(max);
    if e < -1023 {
        values.iter_mut().for_each(|v| *v /= f64::MIN_POSITIVE);
        return f64::MIN_POSITIVE * scaled(values, max / f64::MIN_POSITIVE);
    }
    let scale = libm::ldexp(1.0, -e);
    let (mut csum, mut frac1, mut frac2) = (1.0, 0.0, 0.0);
    for v in values.iter() {
        let (square, square_lo) = product(v * scale, v * scale);
        let (sum, sum_lo) = fast_sum(csum, square);
        csum = sum;
        frac1 += square_lo;
        frac2 += sum_lo;
    }
    let h = libm::sqrt(csum - 1.0 + (frac1 + frac2));
    let (square, square_lo) = product(-h, h);
    let (sum, sum_lo) = fast_sum(csum, square);
    let x = sum - 1.0 + ((frac1 + square_lo) + (frac2 + sum_lo));
    (h + x / (2.0 * h)) / scale
}

// A product and its rounding error, which fma recovers exactly.
fn product(x: f64, y: f64) -> (f64, f64) {
    let p = x * y;
    (p, libm::fma(x, y, -p))
}

// A sum and its rounding error, exact while a is the larger.
fn fast_sum(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    (s, (a - s) + b)
}

/* Python's fsum, exact partial sums rounded once at the end. */
pub fn exact_sum(values: &[f64]) -> Result<f64> {
    let mut partials: Vec<f64> = Vec::new();
    let (mut special, mut infinities) = (0.0, 0.0);
    for &value in values {
        let mut x = value;
        let mut kept = 0;
        for j in 0..partials.len() {
            let mut y = partials[j];
            if x.abs() < y.abs() {
                core::mem::swap(&mut x, &mut y);
            }
            let hi = x + y;
            let lo = y - (hi - x);
            if lo != 0.0 {
                partials[kept] = lo;
                kept += 1;
            }
            x = hi;
        }
        partials.truncate(kept);
        if x.is_finite() {
            if x != 0.0 {
                partials.push(x);
            }
        } else {
            // A finite value summing to infinity overflowed, and infinities and NaN are summed apart.
            if value.is_finite() {
                return Err(raise("OverflowError", "intermediate overflow in fsum"));
            }
            if value.is_infinite() {
                infinities += value;
            }
            special += value;
            partials.clear();
        }
    }
    if special != 0.0 {
        return if infinities.is_nan() { Err(Error::Value(String::from("-inf + inf in fsum"))) } else { Ok(special) };
    }
    let Some(mut hi) = partials.pop() else { return Ok(0.0) };
    let mut lo = 0.0;
    while let Some(y) = partials.pop() {
        let x = hi;
        hi = x + y;
        lo = y - (hi - x);
        if lo != 0.0 {
            break;
        }
    }
    // Half to even across partials, so the next one left can still tip the rounding.
    if let Some(&next) = partials.last()
        && ((lo < 0.0 && next < 0.0) || (lo > 0.0 && next > 0.0))
    {
        let y = lo * 2.0;
        let x = hi + y;
        if y == x - hi {
            hi = x;
        }
    }
    Ok(hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fsum_is_exact_and_rounds_half_to_even_like_python() {
        assert_eq!(exact_sum(&[0.1; 10]).unwrap(), 1.0);
        assert_eq!(exact_sum(&[1e100, 1.0, -1e100, 1e-100, 1e50, -1.0, -1e50]).unwrap(), 1e-100);
        assert_eq!(exact_sum(&[1e-16, 1.0, 1e16]).unwrap(), 1.0000000000000002e16);
        assert!(exact_sum(&[1e308, 1e308]).is_err() && exact_sum(&[f64::INFINITY, f64::NEG_INFINITY]).is_err());
    }

    #[test]
    fn norm_matches_python_to_the_last_bit() {
        assert_eq!(norm(alloc::vec![2.0, 3.0]), 3.605551275463989);
        assert_eq!(norm(alloc::vec![1e308, 1e308]), 1.4142135623730951e308);
        assert_eq!(norm(alloc::vec![5e-324, 5e-324]), 5e-324);
        assert_eq!(norm(alloc::vec![f64::INFINITY, f64::NAN]), f64::INFINITY);
    }
}
