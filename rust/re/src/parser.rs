use alloc::{boxed::Box, format, string::String, vec, vec::Vec};
use super::ast::*;
use super::matcher::fixed_len;

// Python caps a repeat count below this, and a larger one overflows.
const MAXREPEAT: u64 = 4_294_967_295;

/* Parse a pattern under its flags into a Program, or fail where Python fails and in its words. */
pub fn parse(pattern: &str, flags: Flags) -> Result<Program, ParseError> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut p = Parser {
        p: &chars,
        pos: 0,
        group_count: 0,
        names: Vec::new(),
        open: Vec::new(),
        widths: vec![None],
        refs: Vec::new(),
        flags,
        global: flags,
    };
    let root = p.alternation(true)?;
    if p.pos != p.p.len() {
        return Err(p.at("unbalanced parenthesis", p.pos));
    }
    // A condition may name a group that opens later, so its number is checked once every group is counted.
    if let Some(&(group, at)) = p.refs.iter().find(|(group, _)| *group > p.group_count) {
        return Err(p.at(format!("invalid group reference {group}"), at));
    }
    Ok(Program { root, group_count: p.group_count, names: p.names, flags: p.global })
}

struct Parser<'a> {
    p: &'a [char],
    pos: usize,
    group_count: usize,
    names: Vec<(String, usize)>,
    open: Vec<usize>, // indexes of groups whose closing paren is still ahead
    widths: Vec<Option<usize>>, // each group's fixed width once it closes
    refs: Vec<(usize, usize)>, // numbered conditions and where each names its group
    flags: Flags, // what the node being parsed is under
    global: Flags,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<char> { self.p.get(self.pos).copied() }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek();
        if c.is_some() { self.pos += 1; }
        c
    }

    fn eat(&mut self, c: char) -> bool {
        let hit = self.peek() == Some(c);
        if hit { self.pos += 1; }
        hit
    }

    fn at(&self, msg: impl Into<String>, pos: usize) -> ParseError { ParseError::At(msg.into(), pos) }

    fn text(&self, from: usize, to: usize) -> String { self.p[from..to].iter().collect() }

    /* Lowest precedence, branches split on the pipe. */
    fn alternation(&mut self, first: bool) -> Result<Node, ParseError> {
        let mut branches = vec![self.concat(first)?];
        while self.eat('|') {
            branches.push(self.concat(false)?);
        }
        Ok(if branches.len() == 1 { branches.pop().unwrap() } else { Node::Alt(branches) })
    }

    /* A run of items until pipe, close paren, or end, each quantifier binding to the item before it. */
    fn concat(&mut self, first: bool) -> Result<Node, ParseError> {
        let mut items = Vec::new();
        loop {
            self.skip_verbose();
            match self.peek() {
                None | Some('|') | Some(')') => break,
                Some('*' | '+' | '?' | '{') => self.quantify(&mut items)?,
                _ => {
                    if let Some(item) = self.atom(first && items.is_empty())? {
                        items.push(item);
                    }
                }
            }
        }
        Ok(match items.len() {
            0 => Node::Empty,
            1 => items.pop().unwrap(),
            _ => Node::Concat(items),
        })
    }

    // Under verbose, whitespace and a comment up to the end of its line stand for nothing.
    fn skip_verbose(&mut self) {
        while self.flags.verbose {
            match self.peek() {
                Some(' ' | '\t' | '\n' | '\r' | '\u{0B}' | '\u{0C}') => self.pos += 1,
                Some('#') => while self.bump().is_some_and(|c| c != '\n') {},
                _ => break,
            }
        }
    }

    // A quantifier repeats the item before it, which may be neither missing, an anchor, nor a repeat already.
    fn quantify(&mut self, items: &mut Vec<Node>) -> Result<(), ParseError> {
        let at = self.pos;
        let (min, max) = match self.bump() {
            Some('*') => (0, None),
            Some('+') => (1, None),
            Some('?') => (0, Some(1)),
            _ => match self.bound()? {
                Some(bound) => bound,
                None => {
                    items.push(Node::Char('{', self.flags));
                    return Ok(());
                }
            },
        };
        match items.last() {
            None | Some(Node::Start(_) | Node::End(_) | Node::StringStart | Node::StringEnd | Node::WordBoundary(_) | Node::NotWordBoundary(_)) => {
                return Err(self.at("nothing to repeat", at));
            }
            Some(Node::Repeat { .. }) => return Err(self.at("multiple repeat", at)),
            _ => {}
        }
        let greedy = !self.eat('?');
        let possessive = greedy && self.eat('+');
        let node = Box::new(items.pop().unwrap());
        items.push(Node::Repeat { node, min, max, greedy, possessive });
        Ok(())
    }

    /* A counted bound after its brace, None when the text only reads as a literal brace. */
    fn bound(&mut self) -> Result<Option<(usize, Option<usize>)>, ParseError> {
        let here = self.pos;
        if self.peek() == Some('}') {
            return Ok(None);
        }
        let lo = self.digits();
        let hi = if self.eat(',') { self.digits() } else { lo.clone() };
        if !self.eat('}') {
            self.pos = here;
            return Ok(None);
        }
        let count = |digits: &str| match digits.parse::<u64>() {
            Ok(n) if n < MAXREPEAT => Ok(n as usize),
            _ => Err(ParseError::Overflow),
        };
        let min = if lo.is_empty() { 0 } else { count(&lo)? };
        let max = if hi.is_empty() { None } else { Some(count(&hi)?) };
        if max.is_some_and(|max| max < min) {
            return Err(self.at("min repeat greater than max repeat", here));
        }
        Ok(Some((min, max)))
    }

    fn digits(&mut self) -> String {
        let from = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) { self.pos += 1; }
        self.text(from, self.pos)
    }

    // One item, None for what stands for nothing, like a comment or global flags.
    fn atom(&mut self, first: bool) -> Result<Option<Node>, ParseError> {
        let f = self.flags;
        Ok(Some(match self.peek().unwrap() {
            '(' => return self.group(first),
            '[' => self.class()?,
            '\\' => self.escape()?,
            '.' => { self.pos += 1; Node::AnyChar(f) }
            '^' => { self.pos += 1; Node::Start(f) }
            '$' => { self.pos += 1; Node::End(f) }
            c => { self.pos += 1; Node::Char(c, f) }
        }))
    }

    fn group(&mut self, first: bool) -> Result<Option<Node>, ParseError> {
        let start = self.pos;
        self.pos += 1;
        if !self.eat('?') {
            return self.capture(start, None).map(Some);
        }
        let Some(c) = self.bump() else { return Err(self.at("unexpected end of pattern", self.pos)) };
        let node = match c {
            ':' => Node::NonCap(Box::new(self.body(start, self.flags)?)),
            '>' => Node::Atomic(Box::new(self.body(start, self.flags)?)),
            '=' | '!' => self.look(start, false, c == '!')?,
            '<' => match self.bump() {
                Some(d @ ('=' | '!')) => self.look(start, true, d == '!')?,
                Some(d) => return Err(self.at(format!("unknown extension ?<{d}"), start + 1)),
                None => return Err(self.at("unexpected end of pattern", self.pos)),
            },
            'P' => match self.bump() {
                Some('<') => {
                    let (name, begin) = self.name('>')?;
                    self.identifier(&name, begin)?;
                    return self.capture(start, Some((name, begin))).map(Some);
                }
                Some('=') => {
                    let (name, begin) = self.name(')')?;
                    self.identifier(&name, begin)?;
                    let index = self.named(&name, begin)?;
                    if self.open.contains(&index) {
                        return Err(self.at("cannot refer to an open group", begin));
                    }
                    Node::Backref(index, self.flags)
                }
                Some(d) => return Err(self.at(format!("unknown extension ?P{d}"), start + 1)),
                None => return Err(self.at("unexpected end of pattern", self.pos)),
            },
            '#' => loop {
                match self.bump() {
                    Some(')') => return Ok(None),
                    Some(_) => {}
                    None => return Err(self.at("missing ), unterminated comment", start)),
                }
            },
            '(' => self.condition(start)?,
            c if is_flag(c) || c == '-' => return self.inline_flags(start, c, first),
            c => return Err(self.at(format!("unknown extension ?{c}"), start + 1)),
        };
        Ok(Some(node))
    }

    // A group's body under the flags it opens with, closed by its own parenthesis.
    fn body(&mut self, start: usize, flags: Flags) -> Result<Node, ParseError> {
        let outer = core::mem::replace(&mut self.flags, flags);
        let node = self.alternation(false);
        self.flags = outer;
        let node = node?;
        if !self.eat(')') {
            return Err(self.at("missing ), unterminated subpattern", start));
        }
        Ok(node)
    }

    // A look-behind steps back a fixed width, so a body without one is refused as Python refuses it.
    fn look(&mut self, start: usize, behind: bool, negative: bool) -> Result<Node, ParseError> {
        let node = self.body(start, self.flags)?;
        let width = if behind {
            fixed_len(&node, &self.widths).ok_or_else(|| ParseError::Bare(String::from("look-behind requires fixed-width pattern")))?
        } else {
            0
        };
        Ok(Node::Look { node: Box::new(node), behind, negative, width })
    }

    /* Assign the index before the body so order matches paren order. */
    fn capture(&mut self, start: usize, name: Option<(String, usize)>) -> Result<Node, ParseError> {
        self.group_count += 1;
        let index = self.group_count;
        self.widths.push(None);
        let name = match name {
            Some((name, begin)) => {
                if let Some(&(_, was)) = self.names.iter().find(|(n, _)| *n == name) {
                    return Err(self.at(format!("redefinition of group name '{name}' as group {index}; was group {was}"), begin));
                }
                self.names.push((name.clone(), index));
                Some(name)
            }
            None => None,
        };
        self.open.push(index);
        let node = self.body(start, self.flags)?;
        self.open.pop();
        self.widths[index] = fixed_len(&node, &self.widths);
        Ok(Node::Group { index, name, node: Box::new(node) })
    }

    /* A group name up to its terminator, and where it begins. */
    fn name(&mut self, term: char) -> Result<(String, usize), ParseError> {
        let begin = self.pos;
        let mut name = String::new();
        loop {
            match self.bump() {
                Some(c) if c == term => break,
                Some(c) => name.push(c),
                None if name.is_empty() => return Err(self.at("missing group name", self.pos)),
                None => return Err(self.at(format!("missing {term}, unterminated name"), begin)),
            }
        }
        if name.is_empty() {
            return Err(self.at("missing group name", self.pos - 1));
        }
        Ok((name, begin))
    }

    // Python wants a group name to read as an identifier.
    fn identifier(&self, name: &str, begin: usize) -> Result<(), ParseError> {
        let mut chars = name.chars();
        let head = chars.next().is_some_and(|c| c == '_' || c.is_alphabetic());
        if head && chars.all(|c| c == '_' || c.is_alphanumeric()) {
            Ok(())
        } else {
            Err(self.at(format!("bad character in group name '{name}'"), begin))
        }
    }

    fn named(&self, name: &str, begin: usize) -> Result<usize, ParseError> {
        let found = self.names.iter().find(|(n, _)| n == name).map(|(_, index)| *index);
        found.ok_or_else(|| self.at(format!("unknown group name '{name}'"), begin))
    }

    /* A conditional group, the yes branch when the group took part and the no branch otherwise. */
    fn condition(&mut self, start: usize) -> Result<Node, ParseError> {
        let (name, begin) = self.name(')')?;
        let group = if name.bytes().all(|b| b.is_ascii_digit()) {
            match name.parse::<usize>() {
                Ok(0) => return Err(self.at("bad group number", begin)),
                Ok(group) => {
                    self.refs.push((group, begin));
                    group
                }
                Err(_) => return Err(self.at(format!("invalid group reference {name}"), begin)),
            }
        } else {
            self.identifier(&name, begin)?;
            self.named(&name, begin)?
        };
        let yes = self.concat(false)?;
        let no = if self.eat('|') {
            let no = self.concat(false)?;
            if self.peek() == Some('|') {
                return Err(self.at("conditional backref with more than two branches", self.pos));
            }
            no
        } else {
            Node::Empty
        };
        if !self.eat(')') {
            return Err(self.at("missing ), unterminated subpattern", start));
        }
        Ok(Node::Cond { group, yes: Box::new(yes), no: Box::new(no) })
    }

    /* Inline flags, global ones at the very start of the pattern and scoped ones over a group's body. */
    fn inline_flags(&mut self, start: usize, mut c: char, at_start: bool) -> Result<Option<Node>, ParseError> {
        let (mut on, mut off) = (String::new(), String::new());
        if c != '-' {
            loop {
                if c == 'L' {
                    return Err(self.at("bad inline flags: cannot use 'L' flag with a str pattern", self.pos));
                }
                on.push(c);
                if on.contains('a') && on.contains('u') {
                    return Err(self.at("bad inline flags: flags 'a', 'u' and 'L' are incompatible", self.pos));
                }
                match self.bump() {
                    Some(d @ (')' | '-' | ':')) => { c = d; break; }
                    Some(d) if is_flag(d) => c = d,
                    Some(d) => return Err(self.at(if d.is_alphabetic() { "unknown flag" } else { "missing -, : or )" }, self.pos - 1)),
                    None => return Err(self.at("missing -, : or )", self.pos)),
                }
            }
        }
        if c == ')' {
            if !at_start {
                return Err(self.at("global flags not at the start of the expression", start));
            }
            for letter in on.chars() {
                self.flags = self.flags.with(letter, true);
                self.global = self.global.with(letter, true);
            }
            return Ok(None);
        }
        if c == '-' {
            c = match self.bump() {
                Some(d) if is_flag(d) => d,
                Some(d) => return Err(self.at(if d.is_alphabetic() { "unknown flag" } else { "missing flag" }, self.pos - 1)),
                None => return Err(self.at("missing flag", self.pos)),
            };
            loop {
                if matches!(c, 'a' | 'u' | 'L') {
                    return Err(self.at("bad inline flags: cannot turn off flags 'a', 'u' and 'L'", self.pos));
                }
                off.push(c);
                match self.bump() {
                    Some(':') => break,
                    Some(d) if is_flag(d) => c = d,
                    Some(d) => return Err(self.at(if d.is_alphabetic() { "unknown flag" } else { "missing :" }, self.pos - 1)),
                    None => return Err(self.at("missing :", self.pos)),
                }
            }
        }
        if on.chars().any(|letter| off.contains(letter)) {
            return Err(self.at("bad inline flags: flag turned on and off", self.pos - 1));
        }
        let flags = on.chars().fold(self.flags, |f, letter| f.with(letter, true));
        let flags = off.chars().fold(flags, |f, letter| f.with(letter, false));
        Ok(Some(Node::NonCap(Box::new(self.body(start, flags)?))))
    }

    /* A bracket set, where a closing bracket right after the opening one is a literal. */
    fn class(&mut self) -> Result<Node, ParseError> {
        let here = self.pos;
        self.pos += 1;
        let negated = self.eat('^');
        let mut items = Vec::new();
        loop {
            let from = self.pos;
            let item = match self.bump() {
                None => return Err(self.at("unterminated character set", here)),
                Some(']') if !items.is_empty() => break,
                Some('\\') => self.class_escape(from)?,
                Some(c) => ClassItem::Ch(c),
            };
            if !self.eat('-') {
                items.push(item);
                continue;
            }
            let to = self.pos;
            let end = match self.bump() {
                None => return Err(self.at("unterminated character set", here)),
                Some(']') => {
                    items.push(item);
                    items.push(ClassItem::Ch('-'));
                    break;
                }
                Some('\\') => self.class_escape(to)?,
                Some(c) => ClassItem::Ch(c),
            };
            match (item, end) {
                (ClassItem::Ch(lo), ClassItem::Ch(hi)) if lo <= hi => items.push(ClassItem::Range(lo, hi)),
                _ => return Err(self.at(format!("bad character range {}-{}", self.text(from, to - 1), self.text(to, self.pos)), from)),
            }
        }
        Ok(Node::Class { items, negated, flags: self.flags })
    }

    fn class_escape(&mut self, start: usize) -> Result<ClassItem, ParseError> {
        let Some(c) = self.bump() else { return Err(self.at("bad escape (end of pattern)", start)) };
        Ok(match c {
            'd' => ClassItem::Digit,
            'D' => ClassItem::NotDigit,
            'w' => ClassItem::Word,
            'W' => ClassItem::NotWord,
            's' => ClassItem::Space,
            'S' => ClassItem::NotSpace,
            _ => ClassItem::Ch(self.literal(start, c, true)?),
        })
    }

    fn escape(&mut self) -> Result<Node, ParseError> {
        let start = self.pos;
        self.pos += 1;
        let Some(c) = self.bump() else { return Err(self.at("bad escape (end of pattern)", start)) };
        let f = self.flags;
        let class = |item| Node::Class { items: vec![item], negated: false, flags: f };
        Ok(match c {
            'A' => Node::StringStart,
            'Z' => Node::StringEnd,
            'b' => Node::WordBoundary(f),
            'B' => Node::NotWordBoundary(f),
            'd' => class(ClassItem::Digit),
            'D' => class(ClassItem::NotDigit),
            'w' => class(ClassItem::Word),
            'W' => class(ClassItem::NotWord),
            's' => class(ClassItem::Space),
            'S' => class(ClassItem::NotSpace),
            '1'..='9' => self.reference(start, c)?,
            _ => Node::Char(self.literal(start, c, false)?, f),
        })
    }

    // Three octal digits make a character, and anything shorter names a group that has already closed.
    fn reference(&mut self, start: usize, first: char) -> Result<Node, ParseError> {
        let mut digits = String::from(first);
        if let Some(d) = self.peek().filter(char::is_ascii_digit) {
            self.pos += 1;
            digits.push(d);
            if octal(first) && octal(d) && self.peek().is_some_and(octal) {
                digits.push(self.bump().unwrap());
                return Ok(Node::Char(self.octal_char(&digits, start)?, self.flags));
            }
        }
        let group: usize = digits.parse().unwrap();
        if group > self.group_count {
            return Err(self.at(format!("invalid group reference {group}"), start + 1));
        }
        if self.open.contains(&group) {
            return Err(self.at("cannot refer to an open group", start));
        }
        Ok(Node::Backref(group, self.flags))
    }

    /* The character an escape stands for, in a set or out of one, or Python's error for one it does not know. */
    fn literal(&mut self, start: usize, c: char, in_class: bool) -> Result<char, ParseError> {
        Ok(match c {
            'a' => '\u{07}',
            'b' => '\u{08}', // reached only inside a set, a boundary outside one
            'f' => '\u{0C}',
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            'v' => '\u{0B}',
            'x' => self.hex(start, 2)?,
            'u' => self.hex(start, 4)?,
            'U' => self.hex(start, 8)?,
            '0'..='7' if in_class || c == '0' => {
                let mut digits = String::from(c);
                while digits.len() < 3 && self.peek().is_some_and(octal) {
                    digits.push(self.bump().unwrap());
                }
                self.octal_char(&digits, start)?
            }
            c if c.is_ascii_alphanumeric() => return Err(self.at(format!("bad escape \\{c}"), start)),
            c => c,
        })
    }

    // Exactly n hex digits after x, u or U, naming a character a string can hold.
    fn hex(&mut self, start: usize, n: usize) -> Result<char, ParseError> {
        let from = self.pos;
        while self.pos - from < n && self.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
            self.pos += 1;
        }
        let escape = self.text(start, self.pos);
        if self.pos - from < n {
            return Err(self.at(format!("incomplete escape {escape}"), start));
        }
        let code = u32::from_str_radix(&self.text(from, self.pos), 16).ok();
        code.and_then(char::from_u32).ok_or_else(|| self.at(format!("bad escape {escape}"), start))
    }

    fn octal_char(&self, digits: &str, start: usize) -> Result<char, ParseError> {
        let value = u32::from_str_radix(digits, 8).unwrap();
        if value > 0o377 {
            return Err(self.at(format!("octal escape value \\{digits} outside of range 0-0o377"), start));
        }
        Ok(char::from_u32(value).unwrap())
    }
}

fn octal(c: char) -> bool {
    ('0'..='7').contains(&c)
}

fn is_flag(c: char) -> bool {
    matches!(c, 'a' | 'i' | 'L' | 'm' | 's' | 'u' | 'x')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error(pattern: &str) -> String {
        match parse(pattern, Flags::default()) {
            Err(ParseError::At(msg, pos)) => format!("{msg} at position {pos}"),
            Err(ParseError::Bare(msg)) => msg,
            Err(ParseError::Overflow) => String::from("overflow"),
            Ok(_) => String::from("ok"),
        }
    }

    #[test]
    fn errors_read_in_python_words_at_python_positions() {
        for (pattern, want) in [
            ("(a", "missing ), unterminated subpattern at position 0"),
            (r"\q", r"bad escape \q at position 0"),
            ("[z-a]", "bad character range z-a at position 1"),
            (r"(a)\12", "invalid group reference 12 at position 4"),
            (r"(\1)", "cannot refer to an open group at position 1"),
            ("a(?i)b", "global flags not at the start of the expression at position 1"),
            ("(?<=a+)b", "look-behind requires fixed-width pattern"),
            ("x{4294967296}", "overflow"),
        ] {
            assert_eq!(error(pattern), want);
        }
    }

    #[test]
    fn python_patterns_parse() {
        for pattern in [r"(a)(?<=\1)", r"\101\Z", "(?i:a)(?-i:b)", "(a)?(?(1)b|c)", "a*+", "(?>a)", "(?x) a # c"] {
            assert_eq!(error(pattern), "ok", "{pattern}");
        }
    }
}
