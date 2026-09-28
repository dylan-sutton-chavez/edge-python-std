use alloc::{boxed::Box, string::String, vec::Vec};

/* The flags a node was parsed under, so a scoped group changes only what it holds. */
#[derive(Clone, Copy, Default)]
pub struct Flags {
    pub ignorecase: bool, // (?i)
    pub multiline: bool, // (?m), anchors match at line boundaries
    pub dotall: bool, // (?s), dot also matches newline
    pub verbose: bool, // (?x), whitespace and comments stand for nothing
    pub ascii: bool, // (?a), classes, boundaries and case stay within ASCII
}

// Pythona value for each flag letter, so flags cross between main.py and the plugin as one int.
const BITS: [(char, u32); 5] = [('i', 2), ('m', 8), ('s', 16), ('x', 64), ('a', 256)];

impl Flags {
    pub fn from_bits(bits: u32) -> Self {
        BITS.iter().fold(Flags::default(), |flags, &(letter, bit)| flags.with(letter, bits & bit != 0))
    }

    pub fn bits(self) -> u32 {
        BITS.iter().filter(|&&(letter, _)| self.has(letter)).map(|&(_, bit)| bit).sum()
    }

    pub fn with(mut self, letter: char, on: bool) -> Self {
        match letter {
            'i' => self.ignorecase = on,
            'm' => self.multiline = on,
            's' => self.dotall = on,
            'x' => self.verbose = on,
            'a' => self.ascii = on,
            _ => {} // 'u' is what a str pattern already is
        }
        self
    }

    fn has(self, letter: char) -> bool {
        match letter { 'i' => self.ignorecase, 'm' => self.multiline, 's' => self.dotall, 'x' => self.verbose, _ => self.ascii }
    }
}

/* One entry inside a bracket set. Predefined classes stay symbolic. */
#[derive(Clone)]
pub enum ClassItem {
    Ch(char),
    Range(char, char),
    Digit,
    NotDigit,
    Word,
    NotWord,
    Space,
    NotSpace,
}

#[derive(Clone)]
pub enum Node {
    Empty,
    Char(char, Flags),
    AnyChar(Flags), // the dot
    Class { items: Vec<ClassItem>, negated: bool, flags: Flags },
    Start(Flags), // caret anchor
    End(Flags), // dollar anchor
    StringStart, // backslash A
    StringEnd, // backslash Z
    WordBoundary(Flags), // backslash b
    NotWordBoundary(Flags), // backslash B
    Group { index: usize, name: Option<String>, node: Box<Node> },
    NonCap(Box<Node>),
    Atomic(Box<Node>), // never backtracked into once it matched
    Concat(Vec<Node>),
    Alt(Vec<Node>),
    Repeat { node: Box<Node>, min: usize, max: Option<usize>, greedy: bool, possessive: bool },
    Backref(usize, Flags),
    Look { node: Box<Node>, behind: bool, negative: bool, width: usize }, // a look-behind steps back its fixed width
    Cond { group: usize, yes: Box<Node>, no: Box<Node> }, // yes when the group took part
}

/* Compiled pattern, the tree plus capture metadata and flags. */
pub struct Program {
    pub root: Node,
    pub group_count: usize,
    pub names: Vec<(String, usize)>, // maps a group name to its index
    pub flags: Flags, // the global flags, what Pattern.flags reports
}

#[derive(Debug)]
pub enum ParseError {
    At(String, usize), // Python's words and the codepoint offset they point at
    Bare(String), // an error Python raises without a position
    Overflow, // a repeat count past what Python's engine holds
}
