use alloc::format;
use alloc::string::{String, ToString};

// `Int`/`Float` carry the source slice for `parse_int`/`parse_float`, `Constant` covers Python tokens `NaN`/`Infinity`/`-Infinity`.
pub enum Token {
    LBrace, LBracket,
    Null, True, False,
    Str(String),
    Int(i128, String),
    Float(f64, String),
    Constant(String),
}

/* A message ready to raise, Python's own placed by line, column and char, or an Edge limit's at its byte. */
pub struct JsonError(pub String);

pub struct Tokenizer<'a> {
    text: &'a str,
    src: &'a [u8],
    pos: usize,
}

impl<'a> Tokenizer<'a> {
    pub fn new(text: &'a str) -> Self {
        Self { text, src: text.as_bytes(), pos: 0 }
    }

    pub fn pos(&self) -> usize { self.pos }

    /* Python's words for a document it cannot read, with the line, column and char json.loads gives. */
    pub fn fail(&self, msg: &str, at: usize) -> JsonError {
        let before = &self.text[..at];
        let line = before.matches('\n').count() + 1;
        let column = before[before.rfind('\n').map_or(0, |nl| nl + 1)..].chars().count() + 1;
        JsonError(format!("{msg}: line {line} column {column} (char {})", before.chars().count()))
    }

    // What only Edge refuses, a number past 128 bits or a lone surrogate, keeps its own words.
    fn cap(&self, msg: &str, at: usize) -> JsonError {
        JsonError(format!("{msg} at byte {at}"))
    }

    /* The next byte past whitespace, left in place for the parser to judge. */
    pub fn peek(&mut self) -> Option<u8> {
        while let Some(b' ' | b'\t' | b'\n' | b'\r') = self.src.get(self.pos) { self.pos += 1; }
        self.src.get(self.pos).copied()
    }

    pub fn bump(&mut self) { self.pos += 1; }

    /* The value that starts here, and Python's "Expecting value" for anything that cannot start one. */
    pub fn value(&mut self) -> Result<Token, JsonError> {
        let rest = match self.peek() {
            Some(b'{') => { self.pos += 1; return Ok(Token::LBrace) }
            Some(b'[') => { self.pos += 1; return Ok(Token::LBracket) }
            Some(b'"') => return self.read_string(),
            _ => &self.src[self.pos..],
        };
        let (len, token) = if rest.starts_with(b"null") { (4, Token::Null) }
            else if rest.starts_with(b"true") { (4, Token::True) }
            else if rest.starts_with(b"false") { (5, Token::False) }
            else if rest.starts_with(b"NaN") { (3, Token::Constant("NaN".to_string())) }
            else if rest.starts_with(b"Infinity") { (8, Token::Constant("Infinity".to_string())) }
            else if rest.starts_with(b"-Infinity") { (9, Token::Constant("-Infinity".to_string())) }
            else { return self.read_number() };
        self.pos += len;
        Ok(token)
    }

    fn read_string(&mut self) -> Result<Token, JsonError> {
        let begin = self.pos;
        self.pos += 1;
        let mut out = String::new();
        loop {
            match self.src.get(self.pos) {
                None => return Err(self.fail("Unterminated string starting at", begin)),
                Some(b'"') => { self.pos += 1; return Ok(Token::Str(out)) }
                Some(b'\\') => self.read_escape(begin, &mut out)?,
                Some(0..=0x1f) => return Err(self.fail("Invalid control character at", self.pos)),
                Some(_) => {
                    // The text is valid UTF-8 already, so a whole character is taken at once.
                    let c = self.text[self.pos..].chars().next().unwrap();
                    out.push(c);
                    self.pos += c.len_utf8();
                }
            }
        }
    }

    fn read_escape(&mut self, begin: usize, out: &mut String) -> Result<(), JsonError> {
        let slash = self.pos;
        let Some(&esc) = self.src.get(slash + 1) else { return Err(self.fail("Unterminated string starting at", begin)) };
        self.pos += 2;
        out.push(match esc {
            b'"' => '"',
            b'\\' => '\\',
            b'/' => '/',
            b'b' => '\u{08}',
            b'f' => '\u{0C}',
            b'n' => '\n',
            b'r' => '\r',
            b't' => '\t',
            b'u' => return self.read_unicode(slash + 1, out),
            _ => return Err(self.fail("Invalid \\escape", slash)),
        });
        Ok(())
    }

    // A high surrogate takes the low one after it, and a surrogate left alone is past what an Edge string holds.
    fn read_unicode(&mut self, u: usize, out: &mut String) -> Result<(), JsonError> {
        let high = self.hex4(u)?;
        let cp = if (0xD800..0xDC00).contains(&high) {
            if self.src.get(self.pos) != Some(&b'\\') || self.src.get(self.pos + 1) != Some(&b'u') {
                return Err(self.cap("unpaired high surrogate", self.pos));
            }
            self.pos += 2;
            let low = self.hex4(self.pos - 1)?;
            if !(0xDC00..0xE000).contains(&low) { return Err(self.cap("invalid low surrogate", self.pos)); }
            0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00)
        } else if (0xDC00..0xE000).contains(&high) {
            return Err(self.cap("unexpected low surrogate", self.pos));
        } else {
            high
        };
        out.push(char::from_u32(cp).unwrap());
        Ok(())
    }

    fn hex4(&mut self, u: usize) -> Result<u32, JsonError> {
        let hex = self.src.get(self.pos..self.pos + 4).filter(|d| d.iter().all(u8::is_ascii_hexdigit)).ok_or_else(|| self.fail("Invalid \\uXXXX escape", u))?;
        self.pos += 4;
        Ok(hex.iter().fold(0, |acc, d| acc * 16 + (*d as char).to_digit(16).unwrap()))
    }

    fn digits(&mut self) {
        while self.src.get(self.pos).is_some_and(u8::is_ascii_digit) { self.pos += 1; }
    }

    /* Python's number grammar, where a fraction or an exponent counts only when whole and a leading zero stands alone. */
    fn read_number(&mut self) -> Result<Token, JsonError> {
        let start = self.pos;
        if self.src.get(self.pos) == Some(&b'-') { self.pos += 1; }
        match self.src.get(self.pos) {
            Some(b'0') => self.pos += 1,
            Some(b'1'..=b'9') => self.digits(),
            _ => return Err(self.fail("Expecting value", start)),
        }
        let mut float = false;
        if self.src.get(self.pos) == Some(&b'.') && self.src.get(self.pos + 1).is_some_and(u8::is_ascii_digit) {
            self.pos += 1;
            self.digits();
            float = true;
        }
        if matches!(self.src.get(self.pos), Some(b'e' | b'E')) {
            let sign = usize::from(matches!(self.src.get(self.pos + 1), Some(b'+' | b'-')));
            if self.src.get(self.pos + 1 + sign).is_some_and(u8::is_ascii_digit) {
                self.pos += 1 + sign;
                self.digits();
                float = true;
            }
        }
        let text = self.text[start..self.pos].to_string();
        if float {
            text.parse::<f64>().map(|f| Token::Float(f, text.clone())).map_err(|_| self.cap("invalid float", start))
        } else {
            text.parse::<i128>().map(|i| Token::Int(i, text.clone())).map_err(|_| self.cap("integer overflow", start))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_number_stops_where_python_stops() {
        for (text, read) in [("0", 1), ("0.5", 3), ("1e5", 3), ("-0.5e+2", 7), ("1.", 1), ("1e", 1), ("1e+", 1), ("01", 1)] {
            let mut t = Tokenizer::new(text);
            assert!(t.value().is_ok(), "{text}");
            assert_eq!(t.pos(), read, "{text}");
        }
        assert!(Tokenizer::new("-").value().is_err());
    }

    #[test]
    fn an_error_counts_characters_not_bytes() {
        let Err(JsonError(message)) = Tokenizer::new("\"é\\q\"").value() else { panic!("an error") };
        assert_eq!(message, "Invalid \\escape: line 1 column 3 (char 2)");
    }
}
