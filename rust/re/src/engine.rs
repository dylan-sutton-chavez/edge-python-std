use alloc::{format, string::String, vec::Vec};
use crate::ast::{Node, Program};
use crate::matcher::{fixed_len, Caps, Matcher};

/* Which anchoring a single match uses. */
pub enum Mode {
    Search,
    Match,
    Full,
}

/* Engine error, the variant decides the host exception kind. */
#[derive(Debug)]
pub enum ReError {
    Syntax(String), // malformed pattern, surfaces as ValueError
    TooComplex(String), // backtracking blew its budget, surfaces as RuntimeError
}

pub struct Regex {
    prog: Program,
}

impl Regex {
    pub fn compile(pattern: &str) -> Result<Regex, ReError> {
        let prog = crate::parser::parse(pattern).map_err(|e| ReError::Syntax(format!("{} at position {}", e.msg, e.pos)))?;
        validate(&prog.root)?;
        Ok(Regex { prog })
    }

    pub fn group_count(&self) -> usize { self.prog.group_count }

    // Each named group with its index, in the order the pattern opens them.
    pub fn names(&self) -> &[(String, usize)] { &self.prog.names }
}

/* Single match in the requested mode, its capture spans in codepoints. */
pub fn find_rx(re: &Regex, text: &str, mode: Mode) -> Result<Option<Caps>, ReError> {
    let chars: Vec<char> = text.chars().collect();
    let m = Matcher::new(&chars, re.prog.flags);
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
    let m = Matcher::new(chars, re.prog.flags);
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
    let chars: Vec<char> = text.chars().collect();
    let repl_chars: Vec<char> = repl.chars().collect();
    let found = all(re, &chars, count)?;
    let mut out = String::new();
    let mut last = 0;
    for caps in &found {
        let (s, e) = caps[0].unwrap();
        out.extend(&chars[last..s]);
        expand(&repl_chars, &chars, caps, &re.prog.names, &mut out)?;
        last = e;
    }
    out.extend(&chars[last..]);
    Ok((out, found.len()))
}

/* Expand a replacement template against the captured groups. */
fn expand(repl: &[char], chars: &[char], caps: &Caps, names: &[(String, usize)], out: &mut String) -> Result<(), ReError> {
    let mut i = 0;
    while i < repl.len() {
        let ch = repl[i];
        if ch != '\\' { out.push(ch); i += 1; continue; }
        i += 1;
        let Some(&n) = repl.get(i) else { return Err(ReError::Syntax(String::from("bad replacement, trailing backslash"))); };
        match n {
            '\\' => { out.push('\\'); i += 1; }
            'n' => { out.push('\n'); i += 1; }
            't' => { out.push('\t'); i += 1; }
            'r' => { out.push('\r'); i += 1; }
            '0'..='9' => {
                let mut num = 0usize;
                while i < repl.len() && repl[i].is_ascii_digit() {
                    num = num * 10 + repl[i].to_digit(10).unwrap() as usize;
                    i += 1;
                }
                push_group(num, chars, caps, out);
            }
            'g' => {
                i += 1;
                if repl.get(i) != Some(&'<') { return Err(ReError::Syntax(String::from("missing < in group reference"))); }
                i += 1;
                let mut name = String::new();
                while i < repl.len() && repl[i] != '>' { name.push(repl[i]); i += 1; }
                if repl.get(i) != Some(&'>') { return Err(ReError::Syntax(String::from("missing > in group reference"))); }
                i += 1;
                let idx = resolve_name(&name, names)?;
                push_group(idx, chars, caps, out);
            }
            other => { out.push('\\'); out.push(other); i += 1; }
        }
    }
    Ok(())
}

fn resolve_name(name: &str, names: &[(String, usize)]) -> Result<usize, ReError> {
    if !name.is_empty() && name.chars().all(|c| c.is_ascii_digit()) {
        return name.parse::<usize>().map_err(|_| ReError::Syntax(String::from("bad group number")));
    }
    names.iter().find(|(nm, _)| nm == name).map(|(_, ix)| *ix).ok_or(ReError::Syntax(String::from("unknown group name")))
}

fn push_group(idx: usize, chars: &[char], caps: &Caps, out: &mut String) {
    if let Some((s, e)) = caps.get(idx).copied().flatten() {
        for ch in &chars[s..e] { out.push(*ch); }
    }
}

/* Signals that backtracking degraded, so the author can rewrite the pattern. */
fn too_complex() -> ReError {
    ReError::TooComplex(String::from("catastrophic backtracking: O(n^2) time or worse on this input, simplify nested quantifiers"))
}

/* Reject lookbehind whose width is not fixed, which the engine cannot run. */
fn validate(node: &Node) -> Result<(), ReError> {
    match node {
        Node::Look { node, behind: true, .. } => {
            if fixed_len(node).is_none() { return Err(ReError::Syntax(String::from("lookbehind requires fixed width"))); }
            validate(node)
        }
        Node::Look { node, .. } => validate(node),
        Node::Concat(v) | Node::Alt(v) => { for n in v { validate(n)?; } Ok(()) }
        Node::Group { node, .. } | Node::NonCap(node) | Node::Repeat { node, .. } => validate(node),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans(pattern: &str, text: &str) -> Vec<(usize, usize)> {
        find_all_rx(&Regex::compile(pattern).unwrap(), text, 0).unwrap().iter().map(|c| c[0].unwrap()).collect()
    }

    // After an empty match the next one may start there only if it is not empty, as in Python.
    #[test]
    fn an_empty_match_never_repeats_where_it_ended() {
        assert_eq!(spans(r"\b|a", "a"), [(0, 0), (0, 1), (1, 1)]);
        assert_eq!(spans("x*", "axb"), [(0, 0), (1, 2), (2, 2), (3, 3)]);
    }

    #[test]
    fn sub_replaces_up_to_its_count_and_says_how_many() {
        let re = Regex::compile(r"(\d)").unwrap();
        assert_eq!(sub_rx(&re, r"<\1>", "a1b2c3", 2).unwrap(), (String::from("a<1>b<2>c3"), 2));
        assert_eq!(sub_rx(&Regex::compile("x*").unwrap(), "-", "abxd", 0).unwrap(), (String::from("-a-b--d-"), 5));
    }
}
