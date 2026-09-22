#[derive(Debug, PartialEq)]
pub enum TokenValue {
    Sym(String),
    Str(String),
    Num(String),
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
        self.cur += 1;
        self.cn += 1;

        while self.cur < code.len() {
            match code[self.cur] {
                '(' => asts.push(self.parse_list(code)?),
                ')' => {
                    // skip right paren
                    self.cur += 1;
                    self.cn += 1;
                    closed = true;
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
            self.cur += 1;
            self.cn += 1;
        }

        let value = code[start_cur..self.cur].iter().collect::<String>();
        let value = if value.parse::<u128>().is_ok() {
            TokenValue::Num(value)
        } else if check_integer(&value) {
            return Err(format!(
                "error: {value} is invalid as a number: {start_ln} line, {start_cn} char"
            ));
        } else if value.parse::<f64>().is_ok() {
            TokenValue::Num(value)
        } else {
            TokenValue::Sym(value)
        };

        Ok(Ast::Leaf(Token {
            value,
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
        self.cur += 1;
        self.cn += 1;

        while self.cur < code.len() {
            match code[self.cur] {
                '\r' | '\n' if escaped => {
                    escaped = false;
                    self.break_line(code);
                }
                'n' if escaped => {
                    buf.push('\n');
                    escaped = false;
                    self.cur += 1;
                    self.cn += 1;
                }
                'r' if escaped => {
                    buf.push('\r');
                    escaped = false;
                    self.cur += 1;
                    self.cn += 1;
                }
                't' if escaped => {
                    buf.push('\t');
                    escaped = false;
                    self.cur += 1;
                    self.cn += 1;
                }
                '\\' if escaped => {
                    buf.push('\\');
                    escaped = false;
                    self.cur += 1;
                    self.cn += 1;
                }
                '0' if escaped => {
                    buf.push('\0');
                    escaped = false;
                    self.cur += 1;
                    self.cn += 1;
                }
                '\'' if escaped => {
                    buf.push('\'');
                    escaped = false;
                    self.cur += 1;
                    self.cn += 1;
                }
                '"' if escaped => {
                    buf.push('"');
                    escaped = false;
                    self.cur += 1;
                    self.cn += 1;
                }
                // TODO: byte escape and unicode escape
                c if escaped => {
                    return Err(format!(
                        "error: invalid escape character '\\{c}' found: {} line, {} char",
                        self.ln, self.cn
                    ));
                }

                // skip right double-quote
                '"' => {
                    self.cur += 1;
                    self.cn += 1;
                    closed = true;
                    break;
                }
                '\\' => {
                    self.cur += 1;
                    self.cn += 1;
                    escaped = true;
                }
                '\r' | '\n' => {
                    buf.push('\n');
                    self.break_line(code);
                }
                c => {
                    buf.push(c);
                    self.cur += 1;
                    self.cn += 1;
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
                ' ' | '\t' => {
                    self.cur += 1;
                    self.cn += 1;
                }
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
            self.cur += 1;
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
}

pub fn parse(code: &str) -> Result<Vec<Ast>, String> {
    Context::default().parse_root(&code.chars().collect::<Vec<char>>())
}

fn check_integer(s: &str) -> bool {
    let s = s.strip_prefix("-").unwrap_or(s);
    s.chars()
        .all(|c| matches!(c, '0' | '1' | '2' | '3' | '4' | '5' | '6' | '7' | '8' | '9'))
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
        let input = "(1 23 (() ab \"foo bar\n \\n\\\"baz\" c 1.23) ()) ()";
        let expected = vec![
            Ast::Edge(vec![
                Ast::Leaf(Token {
                    value: TokenValue::Num("1".to_string()),
                    ln: 1,
                    cn: 2,
                }),
                Ast::Leaf(Token {
                    value: TokenValue::Num("23".to_string()),
                    ln: 1,
                    cn: 4,
                }),
                Ast::Edge(vec![
                    Ast::Edge(vec![]),
                    Ast::Leaf(Token {
                        value: TokenValue::Sym("ab".to_string()),
                        ln: 1,
                        cn: 11,
                    }),
                    Ast::Leaf(Token {
                        value: TokenValue::Str("foo bar\n \n\"baz".to_string()),
                        ln: 1,
                        cn: 14,
                    }),
                    Ast::Leaf(Token {
                        value: TokenValue::Sym("c".to_string()),
                        ln: 2,
                        cn: 11,
                    }),
                    Ast::Leaf(Token {
                        value: TokenValue::Num("1.23".to_string()),
                        ln: 2,
                        cn: 13,
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
