pub mod token;

use crate::token::*;
use luaugsl_core::error::{CompileError, SourcePos};

pub struct Lexer<'a> {
    input: &'a str,
    bytes: &'a [u8],
    pos: usize,
    line: u32,
    col: u32,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        // Skip BOM if present
        let input = input.strip_prefix('\u{FEFF}').unwrap_or(input);
        Lexer {
            input,
            bytes: input.as_bytes(),
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    pub fn tokenize(mut self) -> Result<Vec<TokenWithPos>, CompileError> {
        let mut tokens = Vec::new();
        loop {
            let (tok, pos) = self.next_token()?;
            let is_eof = tok == Token::Eof;
            tokens.push(TokenWithPos { token: tok, pos });
            if is_eof {
                break;
            }
        }
        Ok(tokens)
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn peek2(&self) -> Option<u8> {
        self.bytes.get(self.pos + 1).copied()
    }

    fn advance(&mut self) -> u8 {
        let ch = self.bytes[self.pos];
        self.pos += 1;
        if ch == b'\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        ch
    }

    fn pos(&self) -> SourcePos {
        SourcePos {
            line: self.line,
            column: self.col,
            offset: self.pos,
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            match ch {
                b' ' | b'\t' | b'\r' | b'\n' => {
                    self.advance();
                }
                _ => break,
            }
        }
    }

    fn next_token(&mut self) -> Result<(Token, SourcePos), CompileError> {
        self.skip_whitespace();

        let pos = self.pos();

        let ch = match self.peek() {
            Some(c) => c,
            None => return Ok((Token::Eof, pos)),
        };

        // Comments
        if ch == b'-' && self.peek2() == Some(b'-') {
            return self.read_comment();
        }

        // String literals
        if ch == b'"' || ch == b'\'' {
            return self.read_string();
        }

        // Long bracket string or comment already handled above
        if ch == b'[' {
            // Could be long bracket string [[...]]
            if self.peek2() == Some(b'[') {
                return self.read_long_string();
            }
        }

        // Backtick interpolated string
        if ch == b'`' {
            return self.read_interpolated_string();
        }

        // Numbers
        if ch.is_ascii_digit() || (ch == b'.' && self.peek2().map_or(false, |c| c.is_ascii_digit())) {
            return self.read_number();
        }

        // Identifiers and keywords
        if ch.is_ascii_alphabetic() || ch == b'_' {
            return self.read_identifier();
        }

        // Attributes @name
        if ch == b'@' {
            return self.read_attribute();
        }

        // Operators and punctuation
        self.advance();
        let tok = match ch {
            b'+' => {
                if self.peek() == Some(b'=') {
                    self.advance();
                    Token::PlusEq
                } else {
                    Token::Plus
                }
            }
            b'-' => {
                if self.peek() == Some(b'=') {
                    self.advance();
                    Token::MinusEq
                } else if self.peek() == Some(b'>') {
                    self.advance();
                    Token::Arrow
                } else {
                    Token::Minus
                }
            }
            b'*' => {
                if self.peek() == Some(b'=') {
                    self.advance();
                    Token::StarEq
                } else {
                    Token::Star
                }
            }
            b'/' => {
                if self.peek() == Some(b'=') {
                    self.advance();
                    Token::SlashEq
                } else if self.peek() == Some(b'/') {
                    self.advance();
                    if self.peek() == Some(b'=') {
                        self.advance();
                        Token::FloorDivEq
                    } else {
                        Token::FloorDiv
                    }
                } else {
                    Token::Slash
                }
            }
            b'%' => {
                if self.peek() == Some(b'=') {
                    self.advance();
                    Token::PercentEq
                } else {
                    Token::Percent
                }
            }
            b'^' => {
                if self.peek() == Some(b'=') {
                    self.advance();
                    Token::CaretEq
                } else {
                    Token::Caret
                }
            }
            b'#' => Token::Hash,
            b'(' => Token::LParen,
            b')' => Token::RParen,
            b'{' => Token::LBrace,
            b'}' => Token::RBrace,
            b'[' => Token::LBracket,
            b']' => Token::RBracket,
            b';' => Token::Semicolon,
            b',' => Token::Comma,
            b':' => {
                if self.peek() == Some(b':') {
                    self.advance();
                    Token::DoubleColon
                } else {
                    Token::Colon
                }
            }
            b'.' => {
                if self.peek() == Some(b'.') {
                    self.advance();
                    if self.peek() == Some(b'.') {
                        self.advance();
                        Token::Ellipsis
                    } else if self.peek() == Some(b'=') {
                        self.advance();
                        Token::ConcatEq
                    } else {
                        Token::Concat
                    }
                } else {
                    Token::Dot
                }
            }
            b'=' => {
                if self.peek() == Some(b'=') {
                    self.advance();
                    Token::EqEq
                } else {
                    Token::Eq
                }
            }
            b'~' => {
                if self.peek() == Some(b'=') {
                    self.advance();
                    Token::TildeEq
                } else {
                    Token::Tilde
                }
            }
            b'|' => Token::Pipe,
            b'&' => Token::Ampersand,
            b'<' => {
                if self.peek() == Some(b'=') {
                    self.advance();
                    Token::Le
                } else {
                    Token::Lt
                }
            }
            b'>' => {
                if self.peek() == Some(b'=') {
                    self.advance();
                    Token::Ge
                } else {
                    Token::Gt
                }
            }
            _ => {
                return Err(CompileError::UnexpectedChar(
                    ch as char,
                    pos,
                ))
            }
        };

        Ok((tok, pos))
    }

    fn read_comment(&mut self) -> Result<(Token, SourcePos), CompileError> {
        let pos = self.pos();
        self.advance(); // -
        self.advance(); // -

        // Check for long comment --[=[ ]=]
        if self.peek() == Some(b'[') {
            let eq_count = self.count_long_bracket_eqs();
            if self.peek() == Some(b'[') {
                self.advance(); // [
                return self.skip_long_comment(eq_count, pos);
            }
        }

        // Single-line comment: skip to end of line
        while let Some(ch) = self.peek() {
            if ch == b'\n' {
                break;
            }
            self.advance();
        }

        // Comments are skipped — return next token
        self.next_token()
    }

    fn count_long_bracket_eqs(&mut self) -> usize {
        let mut count = 0;
        while self.peek() == Some(b'=') {
            self.advance();
            count += 1;
        }
        count
    }

    fn skip_long_comment(&mut self, eq_count: usize, _pos: SourcePos) -> Result<(Token, SourcePos), CompileError> {
        loop {
            match self.peek() {
                Some(b']') => {
                    self.advance();
                    let close_count = self.count_long_bracket_eqs();
                    if close_count == eq_count && self.peek() == Some(b']') {
                        self.advance();
                        return self.next_token();
                    }
                }
                Some(_) => {
                    self.advance();
                }
                None => {
                    // Unterminated comment is OK at EOF
                    return Ok((Token::Eof, self.pos()));
                }
            }
        }
    }

    fn read_string(&mut self) -> Result<(Token, SourcePos), CompileError> {
        let pos = self.pos();
        let quote = self.advance(); // " or '
        let mut s = String::new();

        while let Some(ch) = self.peek() {
            if ch == quote {
                self.advance();
                return Ok((Token::String(s), pos));
            }
            if ch == b'\\' {
                self.advance();
                match self.peek() {
                    Some(b'n') => { s.push('\n'); self.advance(); }
                    Some(b't') => { s.push('\t'); self.advance(); }
                    Some(b'r') => { s.push('\r'); self.advance(); }
                    Some(b'\\') => { s.push('\\'); self.advance(); }
                    Some(b'"') => { s.push('"'); self.advance(); }
                    Some(b'\'') => { s.push('\''); self.advance(); }
                    Some(b'0') => { s.push('\0'); self.advance(); }
                    Some(b'u') => {
                        self.advance();
                        if self.peek() == Some(b'{') {
                            self.advance();
                            let mut hex = String::new();
                            while let Some(h) = self.peek() {
                                if h == b'}' { break; }
                                hex.push(h as char);
                                self.advance();
                            }
                            if self.peek() == Some(b'}') {
                                self.advance();
                            }
                            if let Ok(codepoint) = u32::from_str_radix(&hex, 16) {
                                if let Some(c) = char::from_u32(codepoint) {
                                    s.push(c);
                                }
                            }
                        }
                    }
                    Some(c) => {
                        s.push(c as char);
                        self.advance();
                    }
                    None => return Err(CompileError::UnterminatedString(pos)),
                }
            } else if ch == b'\n' {
                return Err(CompileError::UnterminatedString(pos));
            } else {
                s.push(ch as char);
                self.advance();
            }
        }

        Err(CompileError::UnterminatedString(pos))
    }

    fn read_long_string(&mut self) -> Result<(Token, SourcePos), CompileError> {
        let pos = self.pos();
        self.advance(); // [
        let eq_count = self.count_long_bracket_eqs();

        if self.peek() != Some(b'[') {
            // Not a valid long bracket, treat as [ token
            return Ok((Token::LBracket, pos));
        }
        self.advance(); // [

        let mut s = String::new();
        loop {
            match self.peek() {
                Some(b']') => {
                    self.advance();
                    let close_count = self.count_long_bracket_eqs();
                    if close_count == eq_count && self.peek() == Some(b']') {
                        self.advance();
                        return Ok((Token::String(s), pos));
                    }
                    s.push(']');
                    for _ in 0..close_count {
                        s.push('=');
                    }
                }
                Some(ch) => {
                    s.push(ch as char);
                    self.advance();
                }
                None => return Err(CompileError::UnterminatedString(pos)),
            }
        }
    }

    fn read_interpolated_string(&mut self) -> Result<(Token, SourcePos), CompileError> {
        let pos = self.pos();
        self.advance(); // `
        let mut parts = Vec::new();
        let mut current = String::new();

        while let Some(ch) = self.peek() {
            if ch == b'`' {
                self.advance();
                if !current.is_empty() {
                    parts.push(StringPart::Literal(current));
                }
                return Ok((Token::Interpolated(parts), pos));
            }
            if ch == b'{' {
                self.advance();
                if !current.is_empty() {
                    parts.push(StringPart::Literal(current));
                    current = String::new();
                }
                // Read the expression inside {}
                let expr_str = self.read_interpolation_expr()?;
                parts.push(StringPart::Expr(expr_str));
            } else if ch == b'\\' {
                self.advance();
                match self.peek() {
                    Some(b'n') => { current.push('\n'); self.advance(); }
                    Some(b't') => { current.push('\t'); self.advance(); }
                    Some(b'`') => { current.push('`'); self.advance(); }
                    Some(b'{') => { current.push('{'); self.advance(); }
                    Some(b'\'') => { current.push('\''); self.advance(); }
                    Some(c) => { current.push(c as char); self.advance(); }
                    None => return Err(CompileError::UnterminatedString(pos)),
                }
            } else {
                current.push(ch as char);
                self.advance();
            }
        }

        Err(CompileError::UnterminatedString(pos))
    }

    fn read_interpolation_expr(&mut self) -> Result<String, CompileError> {
        let mut depth = 1;
        let mut s = String::new();
        while let Some(ch) = self.peek() {
            if ch == b'{' {
                depth += 1;
                s.push(ch as char);
                self.advance();
            } else if ch == b'}' {
                depth -= 1;
                if depth == 0 {
                    self.advance();
                    return Ok(s.trim().to_string());
                }
                s.push(ch as char);
                self.advance();
            } else if ch == b'`' || ch == b'\n' {
                return Ok(s.trim().to_string());
            } else {
                s.push(ch as char);
                self.advance();
            }
        }
        Ok(s.trim().to_string())
    }

    fn read_number(&mut self) -> Result<(Token, SourcePos), CompileError> {
        let pos = self.pos();
        let start = self.pos;
        let mut is_float = false;
        let mut suffix = None;

        // Hexadecimal
        if self.peek() == Some(b'0') {
            let ch2 = self.peek2();
            if ch2 == Some(b'x') || ch2 == Some(b'X') {
                self.advance(); // 0
                self.advance(); // x
                while let Some(ch) = self.peek() {
                    if ch.is_ascii_hexdigit() || ch == b'_' {
                        self.advance();
                    } else {
                        break;
                    }
                }
                let s = &self.input[start..self.pos];
                let clean = s.replace("_", "");
                if let Ok(n) = u64::from_str_radix(&clean[2..], 16) {
                    return Ok((Token::Number(n as f64), pos));
                } else {
                    return Err(CompileError::InvalidNumber(s.to_string(), pos));
                }
            }
            // Binary
            if ch2 == Some(b'b') || ch2 == Some(b'B') {
                self.advance(); // 0
                self.advance(); // b
                while let Some(ch) = self.peek() {
                    if ch == b'0' || ch == b'1' || ch == b'_' {
                        self.advance();
                    } else {
                        break;
                    }
                }
                let s = &self.input[start..self.pos];
                let clean = s.replace("_", "");
                if let Ok(n) = u64::from_str_radix(&clean[2..], 2) {
                    return Ok((Token::Number(n as f64), pos));
                } else {
                    return Err(CompileError::InvalidNumber(s.to_string(), pos));
                }
            }
        }

        // Decimal or float
        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() {
                self.advance();
            } else if ch == b'.' && !is_float && self.peek2().map_or(false, |c| c.is_ascii_digit()) {
                is_float = true;
                self.advance();
            } else if ch == b'_' {
                self.advance();
            } else {
                break;
            }
        }

        // Scientific notation
        if let Some(ch) = self.peek() {
            if ch == b'e' || ch == b'E' {
                let e_pos = self.pos;
                self.advance();
                if let Some(c) = self.peek() {
                    if c == b'+' || c == b'-' {
                        self.advance();
                    }
                }
                let mut has_digits = false;
                while let Some(c) = self.peek() {
                    if c.is_ascii_digit() {
                        self.advance();
                        has_digits = true;
                    } else {
                        break;
                    }
                }
                if !has_digits {
                    // Rollback
                    self.pos = e_pos;
                }
            }
        }

        // Suffix
        if let Some(ch) = self.peek() {
            if ch == b'f' || ch == b'F' {
                suffix = Some('f');
                self.advance();
            } else if ch == b'u' || ch == b'U' {
                suffix = Some('u');
                self.advance();
            }
        }

        let s = &self.input[start..self.pos];
        let clean = s.replace("_", "");
        let clean = clean.trim_end_matches(|c: char| c == 'f' || c == 'F' || c == 'u' || c == 'U');

        if let Some('u') = suffix {
            if let Ok(n) = clean.parse::<u64>() {
                return Ok((Token::Number(n as f64), pos));
            }
        }

        match clean.parse::<f64>() {
            Ok(n) => Ok((Token::Number(n), pos)),
            Err(_) => Err(CompileError::InvalidNumber(s.to_string(), pos)),
        }
    }

    fn read_identifier(&mut self) -> Result<(Token, SourcePos), CompileError> {
        let pos = self.pos();
        let start = self.pos;

        while let Some(ch) = self.peek() {
            if ch.is_ascii_alphanumeric() || ch == b'_' {
                self.advance();
            } else {
                break;
            }
        }

        let s = &self.input[start..self.pos];
        let tok = match s {
            // Lua/Luau keywords
            "and" => Token::And,
            "break" => Token::Break,
            "do" => Token::Do,
            "else" => Token::Else,
            "elseif" => Token::Elseif,
            "end" => Token::End,
            "false" => Token::False,
            "for" => Token::For,
            "function" => Token::Function,
            "if" => Token::If,
            "in" => Token::In,
            "local" => Token::Local,
            "nil" => Token::Nil,
            "not" => Token::Not,
            "or" => Token::Or,
            "repeat" => Token::Repeat,
            "return" => Token::Return,
            "then" => Token::Then,
            "true" => Token::True,
            "until" => Token::Until,
            "while" => Token::While,
            "const" => Token::Const,
            "type" => Token::Type,
            "export" => Token::Export,
            "continue" => Token::Continue,
            "import" => Token::Import, // Reserved but gives error

            // Shader-specific keywords
            "uniform" => Token::Uniform,
            "sampler" => Token::Sampler,
            "f32" => Token::F32,
            "i32" => Token::I32,
            "u32" => Token::U32,
            "bool" => Token::Bool,
            "boolean" => Token::Bool,
            "number" => Token::Ident(s.to_string()),
            "vector2" => Token::Vector2,
            "vector3" => Token::Vector3,
            "vector4" => Token::Vector4,
            "vector2i" => Token::Vector2i,
            "vector3i" => Token::Vector3i,
            "vector4i" => Token::Vector4i,
            "vector2u" => Token::Vector2u,
            "vector3u" => Token::Vector3u,
            "vector4u" => Token::Vector4u,
            "bvector2" => Token::BVector2,
            "bvector3" => Token::BVector3,
            "bvector4" => Token::BVector4,
            "mat2x2" => Token::Mat2x2,
            "mat3x3" => Token::Mat3x3,
            "mat4x4" => Token::Mat4x4,
            "texture2d" => Token::Texture2D,
            "texture3d" => Token::Texture3D,
            "textureCube" => Token::TextureCube,
            "texture2dArray" => Token::Texture2DArray,
            "textureCubeArray" => Token::TextureCubeArray,
            "storageBuffer" => Token::StorageBuffer,
            "storageImage" => Token::StorageImage,

            // Builtins
            "workgroupBarrier" => Token::WorkgroupBarrier,
            "memoryBarrier" => Token::MemoryBarrier,
            "storageBarrier" => Token::StorageBarrier,
            "textureBarrier" => Token::TextureBarrier,

            _ => Token::Ident(s.to_string()),
        };

        Ok((tok, pos))
    }

    fn read_attribute(&mut self) -> Result<(Token, SourcePos), CompileError> {
        let pos = self.pos();
        self.advance(); // @
        let start = self.pos;

        while let Some(ch) = self.peek() {
            if ch.is_ascii_alphanumeric() || ch == b'_' {
                self.advance();
            } else {
                break;
            }
        }

        let name = self.input[start..self.pos].to_string();
        Ok((Token::Attribute(name), pos))
    }
}

/// Token with its source position.
#[derive(Debug, Clone, PartialEq)]
pub struct TokenWithPos {
    pub token: Token,
    pub pos: SourcePos,
}

/// String parts for interpolated strings (lexer-level).
#[derive(Debug, Clone, PartialEq)]
pub enum StringPart {
    Literal(String),
    Expr(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keywords() {
        let input = "local function if then else end for while do return break continue const type export nil true false and or not in repeat until";
        let tokens = Lexer::new(input).tokenize().unwrap();
        assert!(tokens.iter().any(|t| t.token == Token::Local));
        assert!(tokens.iter().any(|t| t.token == Token::Function));
        assert!(tokens.iter().any(|t| t.token == Token::End));
    }

    #[test]
    fn test_numbers() {
        let cases = [
            ("42", 42.0),
            ("3.14159", 3.14159),
            ("6.02e23", 6.02e23),
            ("0x2A", 42.0),
            ("0b101010", 42.0),
            ("3.14f", 3.14),
            ("42u", 42.0),
        ];
        for (input, expected) in cases {
            let tokens = Lexer::new(input).tokenize().unwrap();
            assert!(
                tokens.iter().any(|t| matches!(t.token, Token::Number(n) if (n - expected).abs() < 1e-6)),
                "Failed for input: {}", input
            );
        }
    }

    #[test]
    fn test_strings() {
        let tokens = Lexer::new("\"hello world\"").tokenize().unwrap();
        assert!(tokens.iter().any(|t| matches!(t.token, Token::String(ref s) if s == "hello world")));
    }

    #[test]
    fn test_operators() {
        let input = "+ - * / // % ^ .. == ~= < > <= >= = :: # . : ; , @stage @binding";
        let tokens = Lexer::new(input).tokenize().unwrap();
        assert!(tokens.iter().any(|t| t.token == Token::Plus));
        assert!(tokens.iter().any(|t| t.token == Token::FloorDiv));
        assert!(tokens.iter().any(|t| t.token == Token::Concat));
        assert!(tokens.iter().any(|t| t.token == Token::EqEq));
        assert!(tokens.iter().any(|t| t.token == Token::TildeEq));
        assert!(tokens.iter().any(|t| t.token == Token::DoubleColon));
        assert!(tokens.iter().any(|t| matches!(t.token, Token::Attribute(ref s) if s == "stage")));
        assert!(tokens.iter().any(|t| matches!(t.token, Token::Attribute(ref s) if s == "binding")));
    }

    #[test]
    fn test_shader_keywords() {
        let input = "uniform texture2d sampler vector3 vector2i mat4x4 f32 i32 u32 storageBuffer storageImage";
        let tokens = Lexer::new(input).tokenize().unwrap();
        assert!(tokens.iter().any(|t| t.token == Token::Uniform));
        assert!(tokens.iter().any(|t| t.token == Token::Texture2D));
        assert!(tokens.iter().any(|t| t.token == Token::Vector3));
        assert!(tokens.iter().any(|t| t.token == Token::Vector2i));
        assert!(tokens.iter().any(|t| t.token == Token::Mat4x4));
        assert!(tokens.iter().any(|t| t.token == Token::F32));
        assert!(tokens.iter().any(|t| t.token == Token::StorageBuffer));
        assert!(tokens.iter().any(|t| t.token == Token::StorageImage));
    }

    #[test]
    fn test_comments_skipped() {
        let tokens = Lexer::new("local x = 1 -- this is a comment\nlocal y = 2").tokenize().unwrap();
        let idents: Vec<_> = tokens.iter().filter_map(|t| match &t.token {
            Token::Ident(s) => Some(s.as_str()),
            _ => None,
        }).collect();
        assert_eq!(idents, vec!["x", "y"]);
    }

    #[test]
    fn test_compound_assignment() {
        let input = "+= -= *= /= //=";
        let tokens = Lexer::new(input).tokenize().unwrap();
        assert!(tokens.iter().any(|t| t.token == Token::PlusEq));
        assert!(tokens.iter().any(|t| t.token == Token::MinusEq));
        assert!(tokens.iter().any(|t| t.token == Token::StarEq));
        assert!(tokens.iter().any(|t| t.token == Token::SlashEq));
        assert!(tokens.iter().any(|t| t.token == Token::FloorDivEq));
    }
}
