use alloc::vec::Vec;
use core::cell::Cell;
use super::ast::*;

/* Capture slots, index 0 is the whole match. */
pub type Caps = Vec<Option<(usize, usize)>>;

/* Bundled repetition spec, keeps the recursive helpers small. */
struct Rep<'a> {
    node: &'a Node,
    min: usize,
    max: Option<usize>,
    greedy: bool,
}

/* Stack allowance per call, Chromium overflows near 3x this. */
const MAX_STACK: usize = if cfg!(target_arch = "wasm32") { 64 * 1024 } else { 512 * 1024 };

/* Backtracking matcher over codepoints, so offsets are Unicode aware. */
pub struct Matcher<'a> {
    input: &'a [char],
    steps: Cell<u64>, // backtracking work counter, cumulative per API call
    budget: u64, // abort once the counter passes this
    stack_base: usize, // caller frame address
    too_deep: Cell<bool>, // a frame crossed MAX_STACK
}

impl<'a> Matcher<'a> {
    pub fn new(input: &'a [char]) -> Self {
        // Linear allowance, legitimate matches stay under it, blowups race past it.
        let budget = 100_000 + 2_000 * input.len() as u64;
        let base = 0u8;
        Self { input, steps: Cell::new(0), budget, stack_base: &base as *const u8 as usize, too_deep: Cell::new(false) }
    }

    /* Budget or stack exhausted over the whole call. */
    pub fn exceeded(&self) -> bool { self.steps.get() > self.budget || self.too_deep.get() }

    /* Leftmost search from the beginning. */
    pub fn search(&self, root: &Node, ngroups: usize) -> Option<Caps> {
        self.search_from(root, ngroups, 0, false)
    }

    /* Leftmost search starting no earlier than `from`, where `must_advance` refuses an empty match at `from`. */
    pub fn search_from(&self, root: &Node, ngroups: usize, from: usize, must_advance: bool) -> Option<Caps> {
        for start in from..=self.input.len() {
            if self.exceeded() { break; } // stop scanning once the budget is gone
            let mut caps: Caps = empty_caps(ngroups);
            let mut found: Option<usize> = None;
            self.m(root, start, &mut caps, &mut |end, _| {
                // An empty match where the last one ended would repeat it, so the matcher backtracks instead.
                if must_advance && start == from && end == start { return false; }
                found = Some(end);
                true
            });
            if let Some(end) = found {
                caps[0] = Some((start, end));
                return Some(caps);
            }
        }
        None
    }

    /* Anchored match at position zero, optionally requiring full consumption. */
    pub fn match_at(&self, root: &Node, ngroups: usize, full: bool) -> Option<Caps> {
        let mut caps = empty_caps(ngroups);
        let mut found: Option<usize> = None;
        let len = self.input.len();
        self.m(root, 0, &mut caps, &mut |end, _| {
            if full && end != len { return false; }
            found = Some(end);
            true
        });
        found.map(|end| { caps[0] = Some((0, end)); caps })
    }

    /* Core dispatch, k is the continuation called with the position after node. */
    fn m(&self, node: &Node, pos: usize, caps: &mut Caps, k: &mut dyn FnMut(usize, &mut Caps) -> bool) -> bool {
        let n = self.steps.get() + 1;
        self.steps.set(n);
        if n > self.budget { return false; } // budget gone, unwind every branch
        let here = 0u8;
        if self.stack_base.abs_diff(&here as *const u8 as usize) > MAX_STACK { self.too_deep.set(true); return false; }
        match node {
            Node::Empty => k(pos, caps),
            Node::Char(..) | Node::AnyChar(_) | Node::Class { .. } => {
                pos < self.input.len() && self.single_match(node, pos) && k(pos + 1, caps)
            }
            Node::Start(f) => self.at_start(pos, f.multiline) && k(pos, caps),
            Node::End(f) => self.at_end(pos, f.multiline) && k(pos, caps),
            Node::StringStart => pos == 0 && k(pos, caps),
            Node::StringEnd => pos == self.input.len() && k(pos, caps),
            Node::WordBoundary(f) => self.boundary(pos, f.ascii) && k(pos, caps),
            Node::NotWordBoundary(f) => !self.boundary(pos, f.ascii) && k(pos, caps),
            Node::Concat(v) => self.m_seq(v, pos, caps, k),
            Node::Alt(v) => {
                for branch in v {
                    if self.m(branch, pos, caps, k) { return true; }
                }
                false
            }
            Node::NonCap(inner) => self.m(inner, pos, caps, k),
            Node::Atomic(inner) => self.commit(caps, k, |caps, found| self.m(inner, pos, caps, found)),
            Node::Group { index, node: inner, .. } => {
                let index = *index;
                let start = pos;
                self.m(inner, pos, caps, &mut |end, caps| {
                    let prev = caps[index];
                    caps[index] = Some((start, end));
                    if k(end, caps) { true } else { caps[index] = prev; false }
                })
            }
            Node::Repeat { node: inner, min, max, greedy, possessive } => {
                let rep = Rep { node: inner, min: *min, max: *max, greedy: *greedy };
                if *possessive {
                    return self.commit(caps, k, |caps, found| self.repeat(&rep, pos, 0, caps, found));
                }
                self.repeat(&rep, pos, 0, caps, k)
            }
            Node::Backref(n, f) => self.backref(*n, *f, pos, caps, k),
            Node::Look { node: inner, behind, negative, width } => {
                self.look(inner, *behind, *negative, *width, pos, caps, k)
            }
            Node::Cond { group, yes, no } => {
                let branch = if caps.get(*group).copied().flatten().is_some() { yes } else { no };
                self.m(branch, pos, caps, k)
            }
        }
    }

    // The first way `run` succeeds is kept and never backtracked into, as atomic groups and possessive repeats want.
    fn commit(&self, caps: &mut Caps, k: &mut dyn FnMut(usize, &mut Caps) -> bool, run: impl FnOnce(&mut Caps, &mut dyn FnMut(usize, &mut Caps) -> bool) -> bool) -> bool {
        let saved = caps.clone();
        let mut end = None;
        run(caps, &mut |e, _| { end = Some(e); true });
        match end {
            Some(e) if k(e, caps) => true,
            _ => { *caps = saved; false }
        }
    }

    /* Sequence walker, threads the continuation across nodes. */
    fn m_seq(&self, nodes: &[Node], pos: usize, caps: &mut Caps, k: &mut dyn FnMut(usize, &mut Caps) -> bool) -> bool {
        match nodes.split_first() {
            None => k(pos, caps),
            Some((first, rest)) => {
                self.m(first, pos, caps, &mut |p, c| self.m_seq(rest, p, c, k))
            }
        }
    }

    /* Repetition. Single codepoint atoms run iteratively to bound recursion. */
    fn repeat(&self, rep: &Rep, pos: usize, count: usize, caps: &mut Caps, k: &mut dyn FnMut(usize, &mut Caps) -> bool) -> bool {
        if is_single(rep.node) {
            return self.repeat_single(rep, pos, caps, k);
        }
        let can_more = rep.max.is_none_or(|m| count < m);
        if rep.greedy {
            if can_more {
                let stepped = self.m(rep.node, pos, caps, &mut |p, c| {
                    if p == pos { return false; } // stop zero width expansion
                    self.repeat(rep, p, count + 1, c, k)
                });
                if stepped { return true; }
            }
            count >= rep.min && k(pos, caps)
        } else {
            if count >= rep.min && k(pos, caps) { return true; }
            if can_more {
                return self.m(rep.node, pos, caps, &mut |p, c| {
                    if p == pos { return false; }
                    self.repeat(rep, p, count + 1, c, k)
                });
            }
            false
        }
    }

    /* Iterative repeat for atoms that consume exactly one codepoint. */
    fn repeat_single(&self, rep: &Rep, pos: usize, caps: &mut Caps, k: &mut dyn FnMut(usize, &mut Caps) -> bool) -> bool {
        let mut n = 0;
        let mut p = pos;
        while rep.max.is_none_or(|m| n < m) && p < self.input.len() && self.single_match(rep.node, p) {
            p += 1;
            n += 1;
        }
        if n < rep.min { return false; }
        if rep.greedy {
            let mut i = n;
            loop {
                if k(pos + i, caps) { return true; }
                if i == rep.min { return false; }
                i -= 1;
            }
        } else {
            let mut i = rep.min;
            loop {
                if k(pos + i, caps) { return true; }
                if i == n { return false; }
                i += 1;
            }
        }
    }

    fn backref(&self, n: usize, f: Flags, pos: usize, caps: &mut Caps, k: &mut dyn FnMut(usize, &mut Caps) -> bool) -> bool {
        match caps.get(n).copied().flatten() {
            None => k(pos, caps), // unmatched group behaves like empty
            Some((s, e)) => {
                let len = e - s;
                if pos + len > self.input.len() { return false; }
                for i in 0..len {
                    if !same(self.input[pos + i], self.input[s + i], f) { return false; }
                }
                k(pos + len, caps)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn look(&self, inner: &Node, behind: bool, negative: bool, width: usize, pos: usize, caps: &mut Caps, k: &mut dyn FnMut(usize, &mut Caps) -> bool) -> bool {
        let mut hit = false;
        if !behind {
            self.m(inner, pos, caps, &mut |_, _| { hit = true; true });
        } else if pos >= width {
            self.m(inner, pos - width, caps, &mut |end, _| { hit = end == pos; hit });
        }
        if hit != negative { k(pos, caps) } else { false }
    }

    /* True when node consumes input[pos] as a single codepoint. */
    fn single_match(&self, node: &Node, pos: usize) -> bool {
        let c = self.input[pos];
        match node {
            Node::Char(want, f) => same(c, *want, *f),
            Node::AnyChar(f) => f.dotall || c != '\n',
            Node::Class { items, negated, flags } => class_hit(items, c, *flags) != *negated,
            _ => false,
        }
    }

    fn at_start(&self, pos: usize, multiline: bool) -> bool {
        pos == 0 || (multiline && self.input[pos - 1] == '\n')
    }

    fn at_end(&self, pos: usize, multiline: bool) -> bool {
        let len = self.input.len();
        if pos == len { return true; }
        if pos == len - 1 && self.input[pos] == '\n' { return true; } // before a trailing newline
        multiline && self.input[pos] == '\n'
    }

    fn boundary(&self, pos: usize, ascii: bool) -> bool {
        let before = pos > 0 && is_word(self.input[pos - 1], ascii);
        let after = pos < self.input.len() && is_word(self.input[pos], ascii);
        before != after
    }
}

fn empty_caps(ngroups: usize) -> Caps {
    let mut v = Vec::with_capacity(ngroups + 1);
    for _ in 0..=ngroups { v.push(None); }
    v
}

fn is_single(node: &Node) -> bool {
    matches!(node, Node::Char(..) | Node::AnyChar(_) | Node::Class { .. })
}

// Equal, or equal once folded under ignorecase, where ascii folds ASCII letters alone.
fn same(a: char, b: char, f: Flags) -> bool {
    a == b || (f.ignorecase && if f.ascii { a.eq_ignore_ascii_case(&b) } else { a.to_lowercase().eq(b.to_lowercase()) })
}

fn is_word(c: char, ascii: bool) -> bool {
    c == '_' || if ascii { c.is_ascii_alphanumeric() } else { c.is_alphanumeric() }
}

/* Predefined class predicates lean on std, so Unicode needs no tables. */
fn item_match(item: &ClassItem, c: char, ascii: bool) -> bool {
    let digit = if ascii { c.is_ascii_digit() } else { c.is_numeric() };
    let space = if ascii { matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{0B}' | '\u{0C}') } else { c.is_whitespace() };
    match item {
        ClassItem::Ch(x) => c == *x,
        ClassItem::Range(lo, hi) => *lo <= c && c <= *hi,
        ClassItem::Digit => digit,
        ClassItem::NotDigit => !digit,
        ClassItem::Word => is_word(c, ascii),
        ClassItem::NotWord => !is_word(c, ascii),
        ClassItem::Space => space,
        ClassItem::NotSpace => !space,
    }
}

/* Class membership, widening by case when ignorecase is set. */
fn class_hit(items: &[ClassItem], c: char, f: Flags) -> bool {
    let contains = |c: char| items.iter().any(|it| item_match(it, c, f.ascii));
    contains(c) || (f.ignorecase && fold_variants(c, f.ascii).into_iter().any(|alt| alt != c && contains(alt)))
}

/* Case variants to test for ignorecase class membership. */
fn fold_variants(c: char, ascii: bool) -> [char; 2] {
    if ascii {
        return [c.to_ascii_lowercase(), c.to_ascii_uppercase()];
    }
    let lo = c.to_lowercase().next().unwrap_or(c);
    let up = c.to_uppercase().next().unwrap_or(c);
    [lo, up]
}

/* Fixed codepoint width of a node, None when it varies, where a reference measures its group. */
pub fn fixed_len(node: &Node, widths: &[Option<usize>]) -> Option<usize> {
    match node {
        Node::Empty | Node::Start(_) | Node::End(_) | Node::StringStart | Node::StringEnd | Node::WordBoundary(_) | Node::NotWordBoundary(_) | Node::Look { .. } => Some(0),
        Node::Char(..) | Node::AnyChar(_) | Node::Class { .. } => Some(1),
        Node::Concat(v) => v.iter().try_fold(0, |total: usize, n| total.checked_add(fixed_len(n, widths)?)),
        Node::Alt(v) => {
            let first = fixed_len(v.first()?, widths)?;
            v.iter().all(|n| fixed_len(n, widths) == Some(first)).then_some(first)
        }
        Node::Group { node, .. } | Node::NonCap(node) | Node::Atomic(node) => fixed_len(node, widths),
        Node::Repeat { node, min, max, .. } => {
            if *max != Some(*min) { return None; }
            fixed_len(node, widths)?.checked_mul(*min)
        }
        Node::Backref(n, _) => widths.get(*n).copied().flatten(),
        Node::Cond { yes, no, .. } => {
            let width = fixed_len(yes, widths)?;
            (fixed_len(no, widths)? == width).then_some(width)
        }
    }
}
