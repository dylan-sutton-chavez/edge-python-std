use alloc::format;
use alloc::string::String;
use wasm_pdk::*;
use crate::{too_large, Integer};

fn negative(name: &str) -> Error {
    Error::Value(format!("{name} must be a non-negative integer"))
}

// The product n * (n - 1) * ... of k factors, which factorial and perm both are.
fn falling(n: i128, k: i128) -> Result<i128> {
    (0..k).try_fold(1i128, |acc, i| acc.checked_mul(n - i).ok_or_else(too_large))
}

fn factorial_of(n: i128) -> Result<i128> {
    if n < 0 {
        return Err(Error::Value(String::from("factorial() not defined for negative values")));
    }
    falling(n, n)
}

fn gcd_of(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

#[plugin_fn]
fn factorial(n: Integer) -> Result<i128> {
    factorial_of(n.0)
}

/* The orderings of k items out of n, or of all n when k is None. */
#[plugin_fn]
fn perm(n: Integer, k: Args) -> Result<i128> {
    if k.len() > 1 {
        return Err(Error::Type(format!("perm expected at most 2 arguments, got {}", k.len() + 1)));
    }
    let Some(Integer(k)) = k.get::<Option<Integer>>(0).transpose()?.flatten() else { return factorial_of(n.0) };
    let n = n.0;
    if n < 0 {
        return Err(negative("n"));
    }
    if k < 0 {
        return Err(negative("k"));
    }
    if k > n { Ok(0) } else { falling(n, k) }
}

/* The ways to choose k items out of n. */
#[plugin_fn]
fn comb(n: Integer, k: Integer) -> Result<i128> {
    let (n, k) = (n.0, k.0);
    if n < 0 {
        return Err(negative("n"));
    }
    if k < 0 {
        return Err(negative("k"));
    }
    if k > n {
        return Ok(0);
    }
    let k = k.min(n - k);
    // After step i the product is C(n - k + i, i), and dividing by the gcd first keeps it exact.
    (1..=k).try_fold(1i128, |acc, i| {
        let g = gcd_of(acc as u128, i as u128) as i128;
        (acc / g).checked_mul((n - k + i) / (i / g)).ok_or_else(too_large)
    })
}

/* The greatest common divisor of any number of integers, gcd() being 0. */
#[plugin_fn]
fn gcd(integers: Args) -> Result<i128> {
    let mut g = 0;
    for h in &integers.0 {
        g = gcd_of(g, Integer::from_handle(h.raw())?.0.unsigned_abs());
    }
    i128::try_from(g).map_err(|_| too_large())
}

/* The least common multiple of any number of integers, lcm() being 1. */
#[plugin_fn]
fn lcm(integers: Args) -> Result<i128> {
    let mut l: u128 = 1;
    for h in &integers.0 {
        let n = Integer::from_handle(h.raw())?.0.unsigned_abs();
        l = if l == 0 || n == 0 { 0 } else { (l / gcd_of(l, n)).checked_mul(n).ok_or_else(too_large)? };
    }
    i128::try_from(l).map_err(|_| too_large())
}

/* The largest integer whose square is at most n. */
#[plugin_fn]
fn isqrt(n: Integer) -> Result<i128> {
    let n = n.0;
    if n < 0 {
        return Err(Error::Value(String::from("isqrt() argument must be nonnegative")));
    }
    if n == 0 {
        return Ok(0);
    }
    // Newton's method from above, where every step stays within n.
    let (mut x, mut y) = (n, n / 2 + n % 2);
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    Ok(x)
}
