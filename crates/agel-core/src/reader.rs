use crate::Expr;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadLimits {
    pub max_source_bytes: usize,
    pub max_depth: usize,
}

impl Default for ReadLimits {
    fn default() -> Self {
        Self {
            max_source_bytes: 1_048_576,
            max_depth: 256,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadError {
    pub offset: usize,
    pub message: String,
}

impl ReadError {
    fn new(offset: usize, message: impl Into<String>) -> Self {
        Self {
            offset,
            message: message.into(),
        }
    }
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "read error at byte {}: {}", self.offset, self.message)
    }
}

impl std::error::Error for ReadError {}

pub fn read_all(source: &str) -> Result<Vec<Expr>, ReadError> {
    read_all_with_limits(source, ReadLimits::default())
}

pub fn read_all_with_limits(source: &str, limits: ReadLimits) -> Result<Vec<Expr>, ReadError> {
    if source.len() > limits.max_source_bytes {
        return Err(ReadError::new(
            limits.max_source_bytes,
            format!("source exceeds limit of {} bytes", limits.max_source_bytes),
        ));
    }
    let mut reader = Reader {
        source: source.as_bytes(),
        offset: 0,
        max_depth: limits.max_depth,
    };
    let mut expressions = Vec::new();
    reader.skip_trivia();
    while !reader.at_end() {
        expressions.push(reader.read_expr(0)?);
        reader.skip_trivia();
    }
    Ok(expressions)
}

struct Reader<'a> {
    source: &'a [u8],
    offset: usize,
    max_depth: usize,
}

impl Reader<'_> {
    fn at_end(&self) -> bool {
        self.offset >= self.source.len()
    }

    fn peek(&self) -> Option<u8> {
        self.source.get(self.offset).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.offset += 1;
        Some(byte)
    }

    fn skip_trivia(&mut self) {
        loop {
            while self.peek().is_some_and(|byte| byte.is_ascii_whitespace()) {
                self.offset += 1;
            }
            if self.peek() == Some(b';') {
                while self.peek().is_some_and(|byte| byte != b'\n') {
                    self.offset += 1;
                }
            } else {
                break;
            }
        }
    }

    fn read_expr(&mut self, depth: usize) -> Result<Expr, ReadError> {
        if depth > self.max_depth {
            return Err(ReadError::new(
                self.offset,
                format!("syntax nesting exceeds limit of {}", self.max_depth),
            ));
        }
        self.skip_trivia();
        let start = self.offset;
        match self.bump() {
            Some(b'(') => {
                if depth >= self.max_depth {
                    Err(ReadError::new(
                        start,
                        format!("syntax nesting exceeds limit of {}", self.max_depth),
                    ))
                } else {
                    self.read_list(start, depth + 1)
                }
            }
            Some(b')') => Err(ReadError::new(start, "unexpected ')'")),
            Some(b'\'') => {
                let quoted = self.read_expr(depth + 1)?;
                Ok(Expr::List(vec![Expr::Symbol("quote".into()), quoted]))
            }
            Some(b'"') => self.read_string(start),
            Some(_) => {
                self.offset = start;
                self.read_atom()
            }
            None => Err(ReadError::new(start, "expected an expression")),
        }
    }

    fn read_list(&mut self, start: usize, depth: usize) -> Result<Expr, ReadError> {
        let mut values = Vec::new();
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(b')') => {
                    self.offset += 1;
                    return Ok(Expr::List(values));
                }
                None => return Err(ReadError::new(start, "unterminated list")),
                _ => values.push(self.read_expr(depth)?),
            }
        }
    }

    fn read_string(&mut self, start: usize) -> Result<Expr, ReadError> {
        let mut value = String::new();
        loop {
            match self.bump() {
                Some(b'"') => return Ok(Expr::String(value)),
                Some(b'\\') => match self.bump() {
                    Some(b'n') => value.push('\n'),
                    Some(b'r') => value.push('\r'),
                    Some(b't') => value.push('\t'),
                    Some(b'"') => value.push('"'),
                    Some(b'\\') => value.push('\\'),
                    Some(other) => {
                        return Err(ReadError::new(
                            self.offset - 1,
                            format!("unknown escape: \\{}", char::from(other)),
                        ));
                    }
                    None => return Err(ReadError::new(start, "unterminated string")),
                },
                Some(byte) if byte.is_ascii() => value.push(char::from(byte)),
                Some(_) => {
                    let character_start = self.offset - 1;
                    let remaining = std::str::from_utf8(&self.source[character_start..])
                        .map_err(|_| ReadError::new(character_start, "string is not UTF-8"))?;
                    let character = remaining
                        .chars()
                        .next()
                        .expect("a non-ASCII byte starts a character");
                    self.offset = character_start + character.len_utf8();
                    value.push(character);
                }
                None => return Err(ReadError::new(start, "unterminated string")),
            }
        }
    }

    fn read_atom(&mut self) -> Result<Expr, ReadError> {
        let start = self.offset;
        while self
            .peek()
            .is_some_and(|byte| !byte.is_ascii_whitespace() && !matches!(byte, b'(' | b')' | b';'))
        {
            self.offset += 1;
        }
        let atom = std::str::from_utf8(&self.source[start..self.offset])
            .map_err(|_| ReadError::new(start, "symbols must be UTF-8"))?;
        match atom {
            "nil" => Ok(Expr::Nil),
            "#t" => Ok(Expr::Bool(true)),
            "#f" => Ok(Expr::Bool(false)),
            _ => atom
                .parse::<i64>()
                .map(Expr::Int)
                .or_else(|_| Ok(Expr::Symbol(atom.to_owned()))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_code_as_data() {
        assert_eq!(
            read_all("'(hello 42 \"world\")").unwrap(),
            vec![Expr::List(vec![
                Expr::Symbol("quote".into()),
                Expr::List(vec![
                    Expr::Symbol("hello".into()),
                    Expr::Int(42),
                    Expr::String("world".into()),
                ]),
            ])]
        );
    }

    #[test]
    fn reports_unclosed_lists() {
        let error = read_all("(one (two)").unwrap_err();
        assert_eq!(error.offset, 0);
    }

    #[test]
    fn reads_unicode_human_text() {
        assert_eq!(
            read_all("\"Ahoj, svet 👋\"").unwrap(),
            vec![Expr::String("Ahoj, svet 👋".into())]
        );
    }

    #[test]
    fn enforces_source_and_depth_limits() {
        let tiny = ReadLimits {
            max_source_bytes: 3,
            max_depth: 1,
        };
        assert!(read_all_with_limits("1234", tiny).is_err());
        assert!(read_all_with_limits(
            "((x))",
            ReadLimits {
                max_source_bytes: 10,
                max_depth: 1
            }
        )
        .is_err());
        assert!(read_all_with_limits(
            "(())",
            ReadLimits {
                max_source_bytes: 10,
                max_depth: 1
            }
        )
        .is_err());
    }
}
