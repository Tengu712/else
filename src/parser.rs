macro_rules! spaces {
    () => {
        ' ' | '\t'
    };
}

macro_rules! newlines {
    () => {
        '\r' | '\n'
    };
}

macro_rules! whitespaces {
    () => {
        spaces!() | newlines!()
    };
}

macro_rules! delimiters {
    () => {
        '(' | ')' | ';' | '"' | '\'' | whitespaces!()
    };
}

use regex::Regex;
use std::sync::LazyLock;

const INT_RE_RAW: &str = r"^[+-]?[0-9][0-9_]*$";
const FLOAT_RE_RAW: &str = r"^[+-]?[0-9][0-9_]*\.[0-9][0-9_]*([eE][+-]?[0-9][0-9_]*)?$";

static INT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(INT_RE_RAW).unwrap());
static FLOAT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(FLOAT_RE_RAW).unwrap());

#[derive(Debug, PartialEq)]
pub enum TokenValue {
    Sym(String),
    Int(u128, bool),
    Float(f64),
    Str(String),
    Char(char),
}

#[derive(Debug, PartialEq)]
pub struct Token {
    pub v: TokenValue,
    pub ln: usize,
    pub cn: usize,
}

impl Token {
    fn try_to_number(s: &str, ln: usize, cn: usize) -> Result<Option<Self>, String> {
        let err = |e: &str| format!("error: {s} is {e} as a number: {ln} line, {cn} char");
        let v = if let Some((v, m)) = check_integer(s).map_err(err)? {
            TokenValue::Int(v, m)
        } else if let Some(v) = check_float(s).map_err(err)? {
            TokenValue::Float(v)
        } else {
            return Ok(None);
        };
        Ok(Some(Self { v, ln, cn }))
    }
}

#[derive(Debug, PartialEq)]
pub enum Ast {
    Edge(Vec<Ast>),
    Leaf(Token),
}

pub fn parse(code: &str) -> Result<Vec<Ast>, String> {
    Context::default().parse_root(&code.chars().collect::<Vec<char>>())
}

struct Context {
    cur: usize,
    ln: usize,
    cn: usize,
}

impl Default for Context {
    fn default() -> Self {
        Self {
            cur: 0,
            ln: 1,
            cn: 1,
        }
    }
}

impl Context {
    fn parse_root(&mut self, code: &[char]) -> Result<Vec<Ast>, String> {
        let mut asts = vec![];

        while self.cur < code.len() {
            match code[self.cur] {
                whitespaces!() => self.skip_whitespaces(code),
                ';' => self.skip_comment(code),
                '(' => asts.push(self.parse_list(code)?),
                _ => {
                    return Err(format!(
                        "syntax error: anything other than whitespace, comment or list not allowed on the top level: {} line, {} char",
                        self.ln, self.cn
                    ));
                }
            }
        }

        Ok(asts)
    }

    fn parse_list(&mut self, code: &[char]) -> Result<Ast, String> {
        let start_ln = self.ln;
        let start_cn = self.cn;
        let mut asts = vec![];

        // skip left paren
        self.advance();

        while self.cur < code.len() {
            match code[self.cur] {
                whitespaces!() => self.skip_whitespaces(code),
                ';' => self.skip_comment(code),
                '(' => asts.push(self.parse_list(code)?),
                ')' => break,
                _ => asts.push(self.parse_token(code)?),
            }
        }

        if self.cur >= code.len() {
            return Err(format!(
                "syntax error: list not closed, the left paren is at: {start_ln} line, {start_cn} char"
            ));
        }

        // skip right paren
        self.advance();

        Ok(Ast::Edge(asts))
    }

    fn parse_token(&mut self, code: &[char]) -> Result<Ast, String> {
        let start_cur = self.cur;
        let start_ln = self.ln;
        let start_cn = self.cn;

        // string literal
        if code[self.cur] == '"' {
            return self.parse_string_literal(code);
        }
        // char literal
        if code[self.cur] == '\'' {
            return self.parse_char_literal(code);
        }

        while self.cur < code.len() {
            if matches!(code[self.cur], delimiters!()) {
                break;
            }
            self.advance();
        }
        let value = code[start_cur..self.cur].iter().collect::<String>();

        if let Some(v) = Token::try_to_number(&value, start_ln, start_cn)? {
            Ok(Ast::Leaf(v))
        } else {
            Ok(Ast::Leaf(Token {
                v: TokenValue::Sym(value),
                ln: start_ln,
                cn: start_cn,
            }))
        }
    }

    fn parse_string_literal(&mut self, code: &[char]) -> Result<Ast, String> {
        let start_ln = self.ln;
        let start_cn = self.cn;
        let mut buf = String::new();

        // skip left double-quote
        self.advance();

        while self.cur < code.len() {
            match code[self.cur] {
                '"' => break,
                '\\' => {
                    if let Some(c) = self.parse_escape_character(code)? {
                        buf.push(c);
                    }
                }
                newlines!() => {
                    buf.push('\n');
                    self.break_line(code);
                }
                c => {
                    buf.push(c);
                    self.advance();
                }
            }
        }

        if self.cur >= code.len() {
            return Err(format!(
                "syntax error: string literal not closed, the left double-quote is at: {start_ln} line, {start_cn} char"
            ));
        }

        // skip right double-quote
        self.advance();

        Ok(Ast::Leaf(Token {
            v: TokenValue::Str(buf),
            ln: start_ln,
            cn: start_cn,
        }))
    }

    fn parse_char_literal(&mut self, code: &[char]) -> Result<Ast, String> {
        let start_ln = self.ln;
        let start_cn = self.cn;
        let err_not_closed = || {
            Err(format!(
                "syntax error: char literal not closed, the left single-quote is at: {start_ln} line, {start_cn} char"
            ))
        };
        let err_must_have_a_char = || {
            Err(format!(
                "syntax error: char literal must have only a character: {start_ln} line, {start_cn} char"
            ))
        };

        // skip left single-quote
        self.advance();

        if self.cur >= code.len() {
            return err_not_closed();
        }

        let c = match code[self.cur] {
            newlines!() => {
                return Err(format!(
                    "syntax error: newline not allowed in char literal: {start_ln} line, {start_cn} char"
                ));
            }
            '\'' => None,
            '\\' => self.parse_escape_character(code)?,
            c => {
                self.advance();
                Some(c)
            }
        };

        if self.cur >= code.len() {
            return err_not_closed();
        }
        let Some(c) = c else {
            return err_must_have_a_char();
        };
        if code[self.cur] != '\'' {
            return err_not_closed();
        }

        // skip right single-quote
        self.advance();

        Ok(Ast::Leaf(Token {
            v: TokenValue::Char(c),
            ln: start_ln,
            cn: start_cn,
        }))
    }

    fn parse_escape_character(&mut self, code: &[char]) -> Result<Option<char>, String> {
        let start_ln = self.ln;
        let start_cn = self.cn;

        // skip backslash
        self.advance();

        if self.cur >= code.len() {
            // NOTE: This is an error but catched as not-closed error.
            return Ok(None);
        }

        let res = match code[self.cur] {
            newlines!() => {
                self.break_line(code);
                self.skip_whitespaces(code);
                return Ok(None);
            }
            'n' => Some('\n'),
            'r' => Some('\r'),
            't' => Some('\t'),
            '\\' => Some('\\'),
            '0' => Some('\0'),
            '\'' => Some('\''),
            '"' => Some('"'),
            // TODO: byte escape and unicode escape
            c => {
                return Err(format!(
                    "error: invalid escape character '\\{c}' found: {start_ln} line, {start_cn} char"
                ));
            }
        };

        self.advance();
        Ok(res)
    }

    fn skip_whitespaces(&mut self, code: &[char]) {
        while self.cur < code.len() {
            match code[self.cur] {
                spaces!() => self.advance(),
                newlines!() => self.break_line(code),
                _ => break,
            }
        }
    }

    fn skip_comment(&mut self, code: &[char]) {
        while self.cur < code.len() {
            if matches!(code[self.cur], newlines!()) {
                self.break_line(code);
                break;
            }
            self.advance();
        }
    }

    fn break_line(&mut self, code: &[char]) {
        match code[self.cur] {
            '\r' => {
                self.cur += 1;
                self.ln += 1;
                self.cn = 1;
                if self.cur < code.len() && code[self.cur] == '\n' {
                    self.cur += 1;
                }
            }
            '\n' => {
                self.cur += 1;
                self.ln += 1;
                self.cn = 1;
            }
            _ => panic!(
                "unexpected error: tried to break a not line: {} line, {} char",
                self.ln, self.cn
            ),
        }
    }

    fn advance(&mut self) {
        self.cur += 1;
        self.cn += 1;
    }
}

fn check_integer(s: &str) -> Result<Option<(u128, bool)>, &'static str> {
    if !INT_RE.is_match(s) {
        return Ok(None);
    }

    let s = s.replace('_', "");
    if s.starts_with('-') {
        s.parse::<i128>()
            .map(|v| Some((v.unsigned_abs(), true)))
            .map_err(|_| "too small")
    } else {
        s.parse::<u128>()
            .map(|v| Some((v, false)))
            .map_err(|_| "too big")
    }
}

fn check_float(s: &str) -> Result<Option<f64>, &'static str> {
    if !FLOAT_RE.is_match(s) {
        return Ok(None);
    }

    let v = s
        .replace('_', "")
        .parse::<f64>()
        .expect("unexpected error: failed to parse float");
    if v.is_infinite() {
        return Err(if v > 0.0 { "too big" } else { "too small" });
    }
    Ok(Some(v))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn parse_empty() {
        let input = "";
        let expected = vec![];

        assert_eq!(parse(input), Ok(expected));
    }

    #[test]
    fn parse_whitespaces() {
        let input = "  \n\t\r \t\r\n\n";
        let expected = vec![];

        assert_eq!(parse(input), Ok(expected));
    }

    #[test]
    fn parse_only_comment() {
        let input = "; this is comment";
        let expected = vec![];

        assert_eq!(parse(input), Ok(expected));
    }

    #[test]
    fn parse_lists() {
        let input = r#"(340282366920938463463374607431768211455 
-170_141_183_460_469_231_731_687_303_715_884_105_728 (() ab "foo bar
 \n\"baz" c 1.23 -1.1_e1_0_) ()) ()"#;
        let expected = vec![
            Ast::Edge(vec![
                Ast::Leaf(Token {
                    v: TokenValue::Int(340282366920938463463374607431768211455, false),
                    ln: 1,
                    cn: 2,
                }),
                Ast::Leaf(Token {
                    v: TokenValue::Int(170141183460469231731687303715884105728, true),
                    ln: 2,
                    cn: 1,
                }),
                Ast::Edge(vec![
                    Ast::Edge(vec![]),
                    Ast::Leaf(Token {
                        v: TokenValue::Sym("ab".to_string()),
                        ln: 2,
                        cn: 58,
                    }),
                    Ast::Leaf(Token {
                        v: TokenValue::Str("foo bar\n \n\"baz".to_string()),
                        ln: 2,
                        cn: 61,
                    }),
                    Ast::Leaf(Token {
                        v: TokenValue::Sym("c".to_string()),
                        ln: 3,
                        cn: 11,
                    }),
                    Ast::Leaf(Token {
                        v: TokenValue::Float(1.23),
                        ln: 3,
                        cn: 13,
                    }),
                    Ast::Leaf(Token {
                        v: TokenValue::Float(-1.1e10),
                        ln: 3,
                        cn: 18,
                    }),
                ]),
                Ast::Edge(vec![]),
            ]),
            Ast::Edge(vec![]),
        ];

        assert_eq!(parse(input), Ok(expected));
    }

    #[test]
    fn parse_number() {
        let input = "(1 -1 340282366920938463463374607431768211455 -170_141_183_460_469_231_731_687_303_715_884_105_728)\n(1.0 -1.0 -1_.1_e1_0_)";
        let expected = vec![
            Ast::Edge(vec![
                Ast::Leaf(Token {
                    v: TokenValue::Int(1, false),
                    ln: 1,
                    cn: 2,
                }),
                Ast::Leaf(Token {
                    v: TokenValue::Int(1, true),
                    ln: 1,
                    cn: 4,
                }),
                Ast::Leaf(Token {
                    v: TokenValue::Int(340282366920938463463374607431768211455, false),
                    ln: 1,
                    cn: 7,
                }),
                Ast::Leaf(Token {
                    v: TokenValue::Int(170141183460469231731687303715884105728, true),
                    ln: 1,
                    cn: 47,
                }),
            ]),
            Ast::Edge(vec![
                Ast::Leaf(Token {
                    v: TokenValue::Float(1.0),
                    ln: 2,
                    cn: 2,
                }),
                Ast::Leaf(Token {
                    v: TokenValue::Float(-1.0),
                    ln: 2,
                    cn: 6,
                }),
                Ast::Leaf(Token {
                    v: TokenValue::Float(-1.1e10),
                    ln: 2,
                    cn: 11,
                }),
            ]),
        ];

        assert_eq!(parse(input), Ok(expected));
    }

    #[test]
    fn parse_string_literal() {
        let input = r#"("foo bar
 \n\"()\
    baz" "hoge")"#;
        let expected = vec![Ast::Edge(vec![
            Ast::Leaf(Token {
                v: TokenValue::Str("foo bar\n \n\"()baz".to_string()),
                ln: 1,
                cn: 2,
            }),
            Ast::Leaf(Token {
                v: TokenValue::Str("hoge".to_string()),
                ln: 3,
                cn: 10,
            }),
        ])];

        assert_eq!(parse(input), Ok(expected));
    }

    #[test]
    fn parse_char_literal() {
        let input = "('a' ' ' '(' '\\'')";
        let expected = vec![Ast::Edge(vec![
            Ast::Leaf(Token {
                v: TokenValue::Char('a'),
                ln: 1,
                cn: 2,
            }),
            Ast::Leaf(Token {
                v: TokenValue::Char(' '),
                ln: 1,
                cn: 6,
            }),
            Ast::Leaf(Token {
                v: TokenValue::Char('('),
                ln: 1,
                cn: 10,
            }),
            Ast::Leaf(Token {
                v: TokenValue::Char('\''),
                ln: 1,
                cn: 14,
            }),
        ])];

        assert_eq!(parse(input), Ok(expected));
    }

    #[test]
    fn error_unclosed_list() {
        let input = "(1 23";
        assert!(parse(input).is_err());
    }

    #[test]
    fn error_invalid_char_on_toplevel() {
        let input = "foo";
        assert!(parse(input).is_err());
    }

    #[test]
    fn error_too_big_integer() {
        let input = "(340282366920938463463374607431768211456)";
        assert!(parse(input).is_err());
    }

    #[test]
    fn error_too_small_integer() {
        let input = "(-170141183460469231731687303715884105729)";
        assert!(parse(input).is_err());
    }

    #[test]
    fn error_unclosed_string_literal() {
        let input = "(\"foo baz)";
        assert!(parse(input).is_err());
    }

    #[test]
    fn error_invalid_escape_character() {
        let input = "(\"\\a\")";
        assert!(parse(input).is_err());
    }

    #[test]
    fn error_unclosed_char_literal() {
        let input = "('a)";
        assert!(parse(input).is_err());
    }

    #[test]
    fn error_empty_char_literal() {
        let input = "('')";
        assert!(parse(input).is_err());
    }

    #[test]
    fn error_multiple_chars_char_literal() {
        let input = "('ab')";
        assert!(parse(input).is_err());
    }

    #[test]
    fn error_newline_char_literal() {
        let input = "('\n')";
        assert!(parse(input).is_err());
    }

    #[test]
    fn error_escaped_newline_char_literal() {
        let input = "('\\\n')";
        assert!(parse(input).is_err());
    }
}
