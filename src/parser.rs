use regex::Regex;
use std::sync::LazyLock;

const INT_RE_RAW: &str = r"^[+-]?[0-9][0-9_]*$";
const FLOAT_RE_RAW: &str = r"^[+-]?[0-9][0-9_]*\.[0-9][0-9_]*(e[+-]?[0-9][0-9_]*)?$";

static INT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(INT_RE_RAW).unwrap());
static FLOAT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(FLOAT_RE_RAW).unwrap());

#[derive(Debug, PartialEq)]
pub enum TokenValue {
    Sym(String),
    Int(u128, bool),
    Flt(f64),
    Str(String),
}

impl TokenValue {
    fn from(s: &str, ln: usize, cn: usize) -> Result<Self, String> {
        let err = |e: &str| format!("error: {s} is {e} as a number: {ln} line, {cn} char");
        let res = if let Some((v, m)) = check_integer(s).map_err(err)? {
            Self::Int(v, m)
        } else if let Some(v) = check_float(s).map_err(err)? {
            Self::Flt(v)
        } else {
            Self::Sym(s.to_string())
        };
        Ok(res)
    }
}

#[derive(Debug, PartialEq)]
pub struct Token {
    value: TokenValue,
    ln: usize,
    cn: usize,
}

#[derive(Debug, PartialEq)]
pub enum Ast {
    Edge(Vec<Ast>),
    Leaf(Token),
}

pub struct Context {
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
                '(' => asts.push(self.parse_list(code)?),
                ' ' | '\t' | '\r' | '\n' => self.skip_whitespaces(code),
                ';' => self.skip_comment(code),
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
        let mut closed = false;

        // skip left paren
        self.advance();

        while self.cur < code.len() {
            match code[self.cur] {
                '(' => asts.push(self.parse_list(code)?),
                ')' => {
                    // skip right paren
                    closed = true;
                    self.advance();
                    break;
                }
                ' ' | '\t' | '\r' | '\n' => self.skip_whitespaces(code),
                ';' => self.skip_comment(code),
                _ => asts.push(self.parse_token(code)?),
            }
        }

        if closed {
            Ok(Ast::Edge(asts))
        } else {
            Err(format!(
                "syntax error: list not closed, the left paren is at: {start_ln} line, {start_cn} char"
            ))
        }
    }

    fn parse_token(&mut self, code: &[char]) -> Result<Ast, String> {
        let start_cur = self.cur;
        let start_ln = self.ln;
        let start_cn = self.cn;

        // string literal
        if code[self.cur] == '"' {
            return self.parse_string_literal(code);
        }

        while self.cur < code.len() {
            if matches!(code[self.cur], '(' | ')' | ' ' | '\t' | '\r' | '\n' | ';') {
                break;
            }
            self.advance();
        }

        let value = code[start_cur..self.cur].iter().collect::<String>();

        Ok(Ast::Leaf(Token {
            value: TokenValue::from(&value, start_ln, start_cn)?,
            ln: start_ln,
            cn: start_cn,
        }))
    }

    fn parse_string_literal(&mut self, code: &[char]) -> Result<Ast, String> {
        let start_ln = self.ln;
        let start_cn = self.cn;
        let mut closed = false;
        let mut escaped = false;
        let mut buf = String::new();

        // skip left double-quote
        self.advance();

        while self.cur < code.len() {
            match code[self.cur] {
                '\r' | '\n' if escaped => {
                    escaped = false;
                    self.break_line(code);
                }
                'n' if escaped => {
                    escaped = false;
                    buf.push('\n');
                    self.advance();
                }
                'r' if escaped => {
                    escaped = false;
                    buf.push('\r');
                    self.advance();
                }
                't' if escaped => {
                    escaped = false;
                    buf.push('\t');
                    self.advance();
                }
                '\\' if escaped => {
                    escaped = false;
                    buf.push('\\');
                    self.advance();
                }
                '0' if escaped => {
                    escaped = false;
                    buf.push('\0');
                    self.advance();
                }
                '\'' if escaped => {
                    escaped = false;
                    buf.push('\'');
                    self.advance();
                }
                '"' if escaped => {
                    escaped = false;
                    buf.push('"');
                    self.advance();
                }
                // TODO: byte escape and unicode escape
                c if escaped => {
                    return Err(format!(
                        "error: invalid escape character '\\{c}' found: {} line, {} char",
                        self.ln, self.cn
                    ));
                }

                '"' => {
                    // skip right double-quote
                    closed = true;
                    self.advance();
                    break;
                }
                '\\' => {
                    escaped = true;
                    self.advance();
                }
                '\r' | '\n' => {
                    buf.push('\n');
                    self.break_line(code);
                }
                c => {
                    buf.push(c);
                    self.advance();
                }
            }
        }

        if closed {
            Ok(Ast::Leaf(Token {
                value: TokenValue::Str(buf),
                ln: start_ln,
                cn: start_cn,
            }))
        } else {
            Err(format!(
                "syntax error: string literal not closed, the left double-quote is at: {start_ln} line, {start_cn} char"
            ))
        }
    }

    fn skip_whitespaces(&mut self, code: &[char]) {
        while self.cur < code.len() {
            match code[self.cur] {
                ' ' | '\t' => self.advance(),
                '\r' | '\n' => self.break_line(code),
                _ => break,
            }
        }
    }

    fn skip_comment(&mut self, code: &[char]) {
        while self.cur < code.len() {
            if code[self.cur] == '\n' {
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

pub fn parse(code: &str) -> Result<Vec<Ast>, String> {
    Context::default().parse_root(&code.chars().collect::<Vec<char>>())
}

fn check_integer(s: &str) -> Result<Option<(u128, bool)>, &'static str> {
    if !INT_RE.is_match(s) {
        return Ok(None);
    }

    let s = s.replace('_', "");
    if s.starts_with('-') {
        s.parse::<i128>()
            .map(|v| Some((v as u128, true)))
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
                    value: TokenValue::Int(340282366920938463463374607431768211455, false),
                    ln: 1,
                    cn: 2,
                }),
                Ast::Leaf(Token {
                    value: TokenValue::Int(170141183460469231731687303715884105728, true),
                    ln: 2,
                    cn: 1,
                }),
                Ast::Edge(vec![
                    Ast::Edge(vec![]),
                    Ast::Leaf(Token {
                        value: TokenValue::Sym("ab".to_string()),
                        ln: 2,
                        cn: 58,
                    }),
                    Ast::Leaf(Token {
                        value: TokenValue::Str("foo bar\n \n\"baz".to_string()),
                        ln: 2,
                        cn: 61,
                    }),
                    Ast::Leaf(Token {
                        value: TokenValue::Sym("c".to_string()),
                        ln: 3,
                        cn: 11,
                    }),
                    Ast::Leaf(Token {
                        value: TokenValue::Flt(1.23),
                        ln: 3,
                        cn: 13,
                    }),
                    Ast::Leaf(Token {
                        value: TokenValue::Flt(-1.1e10),
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
    #[should_panic]
    fn error_unclosed_list() {
        let input = "(1 23";
        parse(input).unwrap();
    }

    #[test]
    #[should_panic]
    fn error_invalid_char_on_toplevel() {
        let input = "foo";
        parse(input).unwrap();
    }

    #[test]
    #[should_panic]
    fn error_too_big_integer() {
        let input = "(340282366920938463463374607431768211456)";
        parse(input).unwrap();
    }

    #[test]
    #[should_panic]
    fn error_unclosed_string_literal() {
        let input = "(\"foo baz)";
        parse(input).unwrap();
    }

    #[test]
    #[should_panic]
    fn error_invalid_escape_character() {
        let input = "(\"\\a\")";
        parse(input).unwrap();
    }
}
