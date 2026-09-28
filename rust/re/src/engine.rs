use alloc::{format, string::String, vec::Vec};
use crate::ast::{Flags, ParseError, Program};
use crate::matcher::{Caps, Matcher};

/* Which anchoring a single match uses. */
pub enum Mode {
    Search,
    Match,
    Full,
}

/* Engine error, the variant decides the host exception kind. */
#[derive(Debug)]
pub enum ReError {
    Syntax(String), // malformed pattern or template, surfaces as ValueError
    TooComplex(String), // backtracking blew its budget, surfaces as RuntimeError
    Overflow(String), // a repeat count too large, surfaces as OverflowError
    Index(String), // a template naming no group, surfaces as IndexError
}

pub struct Regex {
    prog: Program,
}

impl Regex {
    pub fn compile(pattern: &str, flags: Flags) -> Result<Regex, ReError> {
        let prog = crate::parser::parse(pattern, flags).map_err(|e| match e {
            ParseError::At(msg, pos) => ReError::Syntax(format!("{msg} at position {pos}")),
            ParseError::Bare(msg) => ReError::Syntax(msg),
            ParseError::Overflow => ReError::Overflow(String::from("the repetition number is too large")),
        })?;
        Ok(Regex { prog })
    }

    pub fn group_count(&self) -> usize { self.prog.group_count }

    // Each named group with its index, in the order the pattern opens them.
    pub fn names(&self) -> &[(String, usize)] { &self.prog.names }

    // The global flags in Python's numbering, those passed in and those the pattern opens with.
    pub fn flags(&self) -> u32 { self.prog.flags.bits() }
}

/* Single match in the requested mode, its capture spans in codepoints. */
pub fn find_rx(re: &Regex, text: &str, mode: Mode) -> Result<Option<Caps>, ReError> {
    let chars: Vec<char> = text.chars().collect();
    let m = Matcher::new(&chars);
    let caps = match mode {
        Mode::Search => m.search(&re.prog.root, re.prog.group_count),
        Mode::Match => m.match_at(&re.prog.root, re.prog.group_count, false),
        Mode::Full => m.match_at(&re.prog.root, re.prog.group_count, true),
    };
    match caps {
        None if m.exceeded() => Err(too_complex()),
        found => Ok(found),
    }
}

/* Every match left to right, at most `limit` unless it is zero, as its capture spans. */
pub fn find_all_rx(re: &Regex, text: &str, limit: usize) -> Result<Vec<Caps>, ReError> {
    let chars: Vec<char> = text.chars().collect();
    all(re, &chars, limit)
}

/* The walk findall, finditer, sub and split share, the next search starts where a match ended. */
fn all(re: &Regex, chars: &[char], limit: usize) -> Result<Vec<Caps>, ReError> {
    let m = Matcher::new(chars);
    let mut out = Vec::new();
    let (mut start, mut empty) = (0, false);
    while start <= chars.len() && (limit == 0 || out.len() < limit) {
        let Some(c) = m.search_from(&re.prog.root, re.prog.group_count, start, empty) else {
            if m.exceeded() { return Err(too_complex()); }
            break;
        };
        let (s, e) = c[0].unwrap();
        (start, empty) = (e, s == e);
        out.push(c);
    }
    Ok(out)
}

/* Replace up to `count` matches, every one when it is zero, and say how many it replaced. */
pub fn sub_rx(re: &Regex, repl: &str, text: &str, count: usize) -> Result<(String, usize), ReError> {
    let pieces = template(&repl.chars().collect::<Vec<_>>(), re)?;
    let chars: Vec<char> = text.chars().collect();
    let found = all(re, &chars, count)?;
    let mut out = String::new();
    let mut last = 0;
    for caps in &found {
        let (s, e) = caps[0].unwrap();
        out.extend(&chars[last..s]);
        for piece in &pieces {
            match piece {
                Piece::Text(t) => out.push_str(t),
                // A group that took no part adds nothing, as in Python.
                Piece::Group(g) => if let Some((gs, ge)) = caps[*g] { out.extend(&chars[gs..ge]) },
            }
        }
        last = e;
    }
    out.extend(&chars[last..]);
    Ok((out, found.len()))
}

enum Piece {
    Text(String),
    Group(usize),
}

/* A replacement template read once as Python reads it, before any match, into text and group numbers. */
fn template(repl: &[char], re: &Regex) -> Result<Vec<Piece>, ReError> {
    let err = |msg: String, pos: usize| ReError::Syntax(format!("{msg} at position {pos}"));
    let group = |g: usize, pos: usize| if g > re.group_count() { Err(err(format!("invalid group reference {g}"), pos)) } else { Ok(Piece::Group(g)) };
    let mut pieces = Vec::new();
    let mut text = String::new();
    let mut i = 0;
    while i < repl.len() {
        if repl[i] != '\\' {
            text.push(repl[i]);
            i += 1;
            continue;
        }
        let start = i;
        let Some(&c) = repl.get(i + 1) else { return Err(err(String::from("bad escape (end of pattern)"), start)) };
        i += 2;
        let piece = match c {
            'g' => {
                if repl.get(i) != Some(&'<') { return Err(err(String::from("missing <"), i)); }
                i += 1;
                let begin = i;
                let end = repl[i..].iter().position(|&ch| ch == '>').map(|at| i + at);
                let name: String = repl[i..end.unwrap_or(repl.len())].iter().collect();
                match end {
                    None if name.is_empty() => return Err(err(String::from("missing group name"), i)),
                    None => return Err(err(String::from("missing >, unterminated name"), begin)),
                    Some(at) if at == begin => return Err(err(String::from("missing group name"), at)),
                    Some(at) => i = at + 1,
                }
                if name.bytes().all(|b| b.is_ascii_digit()) {
                    group(name.parse().unwrap_or(usize::MAX), begin)?
                } else {
                    let mut chars = name.chars();
                    let head = chars.next().is_some_and(|ch| ch == '_' || ch.is_alphabetic());
                    if !head || !chars.all(|ch| ch == '_' || ch.is_alphanumeric()) {
                        return Err(err(format!("bad character in group name '{name}'"), begin));
                    }
                    let named = re.names().iter().find(|(n, _)| *n == name).map(|(_, g)| *g);
                    Piece::Group(named.ok_or_else(|| ReError::Index(format!("unknown group name '{name}'")))?)
                }
            }
            '0'..='9' => {
                let mut digits = String::from(c);
                let octal = |ch: Option<&char>| ch.is_some_and(|ch| ('0'..='7').contains(ch));
                if c == '0' {
                    while digits.len() < 3 && octal(repl.get(i)) { digits.push(repl[i]); i += 1; }
                    text.push(char::from((u32::from_str_radix(&digits, 8).unwrap() & 0xff) as u8));
                    continue;
                }
                if repl.get(i).is_some_and(char::is_ascii_digit) {
                    digits.push(repl[i]);
                    i += 1;
                    // Three octal digits are a character, and anything shorter names a group.
                    if octal(Some(&c)) && octal(digits.chars().nth(1).as_ref()) && octal(repl.get(i)) {
                        digits.push(repl[i]);
                        i += 1;
                        let value = u32::from_str_radix(&digits, 8).unwrap();
                        if value > 0o377 {
                            return Err(err(format!("octal escape value \\{digits} outside of range 0-0o377"), start));
                        }
                        text.push(char::from_u32(value).unwrap());
                        continue;
                    }
                }
                group(digits.parse().unwrap(), start + 1)?
            }
            _ => {
                match c {
                    'a' => text.push('\u{07}'),
                    'b' => text.push('\u{08}'),
                    'f' => text.push('\u{0C}'),
                    'n' => text.push('\n'),
                    'r' => text.push('\r'),
                    't' => text.push('\t'),
                    'v' => text.push('\u{0B}'),
                    '\\' => text.push('\\'),
                    c if c.is_ascii_alphabetic() => return Err(err(format!("bad escape \\{c}"), start)),
                    c => { text.push('\\'); text.push(c); }
                }
                continue;
            }
        };
        if !text.is_empty() {
            pieces.push(Piece::Text(core::mem::take(&mut text)));
        }
        pieces.push(piece);
    }
    if !text.is_empty() {
        pieces.push(Piece::Text(text));
    }
    Ok(pieces)
}

/* Signals that backtracking degraded, so the author can rewrite the pattern. */
fn too_complex() -> ReError {
    ReError::TooComplex(String::from("catastrophic backtracking: O(n^2) time or worse on this input, simplify nested quantifiers"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compiled(pattern: &str) -> Regex {
        Regex::compile(pattern, Flags::default()).unwrap()
    }

    fn spans(pattern: &str, text: &str) -> Vec<(usize, usize)> {
        find_all_rx(&compiled(pattern), text, 0).unwrap().iter().map(|c| c[0].unwrap()).collect()
    }

    // After an empty match the next one may start there only if it is not empty, as in Python.
    #[test]
    fn an_empty_match_never_repeats_where_it_ended() {
        assert_eq!(spans(r"\b|a", "a"), [(0, 0), (0, 1), (1, 1)]);
        assert_eq!(spans("x*", "axb"), [(0, 0), (1, 2), (2, 2), (3, 3)]);
    }

    #[test]
    fn sub_replaces_up_to_its_count_and_says_how_many() {
        let re = compiled(r"(\d)");
        assert_eq!(sub_rx(&re, r"<\1>", "a1b2c3", 2).unwrap(), (String::from("a<1>b<2>c3"), 2));
        assert_eq!(sub_rx(&compiled("x*"), "-", "abxd", 0).unwrap(), (String::from("-a-b--d-"), 5));
    }

    // Python reads the template before it looks for a match, so a bad one fails on text it never matches.
    #[test]
    fn a_template_fails_before_any_match() {
        assert!(matches!(sub_rx(&compiled("(a)"), r"\9", "xyz", 0), Err(ReError::Syntax(m)) if m == "invalid group reference 9 at position 1"));
        assert!(matches!(sub_rx(&compiled("a"), r"\g<x>", "a", 0), Err(ReError::Index(m)) if m == "unknown group name 'x'"));
        assert_eq!(sub_rx(&compiled("a"), r"\101\-", "a", 0).unwrap().0, "A\\-");
    }
}
