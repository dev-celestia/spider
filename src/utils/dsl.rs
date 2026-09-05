//! Native approximation of the reference crawler's `-mdc` / `-fdc` DSL match/filter
//! conditions (celestia delegates to `projectdiscovery/dsl`).
//!
//! Supported grammar over the serialized result JSON:
//! - dotted identifiers: `url`, `request.endpoint`, `response.status_code`, ...
//! - literals: strings ('..'/".."), integers, floats, true/false
//! - comparisons: == != < > <= >=
//! - boolean ops: && || !
//! - functions: contains(a,b), starts_with(a,b), ends_with(a,b),
//!   matches(regex,s), upper(s), lower(s), len(x), words(s), lines(s)
//! - parentheses

use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Ident(String),
    Str(String),
    Num(f64),
    Bool(bool),
    Op(String),
    LParen,
    RParen,
    Comma,
}

fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            ' ' | '\t' => i += 1,
            '(' => {
                tokens.push(Token::LParen);
                i += 1;
            }
            ')' => {
                tokens.push(Token::RParen);
                i += 1;
            }
            ',' => {
                tokens.push(Token::Comma);
                i += 1;
            }
            '\'' | '"' => {
                let quote = c;
                i += 1;
                let mut s = String::new();
                while i < chars.len() && chars[i] != quote {
                    s.push(chars[i]);
                    i += 1;
                }
                if i >= chars.len() {
                    return Err("unterminated string literal".into());
                }
                i += 1;
                tokens.push(Token::Str(s));
            }
            '=' if i + 1 < chars.len() && chars[i + 1] == '=' => {
                tokens.push(Token::Op("==".into()));
                i += 2;
            }
            '!' if i + 1 < chars.len() && chars[i + 1] == '=' => {
                tokens.push(Token::Op("!=".into()));
                i += 2;
            }
            '<' if i + 1 < chars.len() && chars[i + 1] == '=' => {
                tokens.push(Token::Op("<=".into()));
                i += 2;
            }
            '>' if i + 1 < chars.len() && chars[i + 1] == '=' => {
                tokens.push(Token::Op(">=".into()));
                i += 2;
            }
            '&' if i + 1 < chars.len() && chars[i + 1] == '&' => {
                tokens.push(Token::Op("&&".into()));
                i += 2;
            }
            '|' if i + 1 < chars.len() && chars[i + 1] == '|' => {
                tokens.push(Token::Op("||".into()));
                i += 2;
            }
            '<' | '>' | '!' => {
                tokens.push(Token::Op(c.to_string()));
                i += 1;
            }
            c if c.is_ascii_digit() => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                let s: String = chars[start..i].iter().collect();
                let n = s
                    .parse::<f64>()
                    .map_err(|_| format!("invalid number literal: {s}"))?;
                tokens.push(Token::Num(n));
            }
            c if c.is_alphabetic() || c == '_' || c == '.' => {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '.') {
                    i += 1;
                }
                let s: String = chars[start..i].iter().collect();
                match s.as_str() {
                    "true" => tokens.push(Token::Bool(true)),
                    "false" => tokens.push(Token::Bool(false)),
                    _ => tokens.push(Token::Ident(s)),
                }
            }
            other => return Err(format!("unexpected character in DSL expression: {other}")),
        }
    }
    Ok(tokens)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    ctx: Value,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    // or_expr := and_expr ('||' and_expr)*
    fn parse_or(&mut self) -> Result<Value, String> {
        let mut left = self.parse_and()?;
        while matches!(self.peek(), Some(Token::Op(op)) if op == "||") {
            self.next();
            let right = self.parse_and()?;
            left = Value::Bool(truthy(&left) || truthy(&right));
        }
        Ok(left)
    }

    // and_expr := unary ('&&' unary)*
    fn parse_and(&mut self) -> Result<Value, String> {
        let mut left = self.parse_unary()?;
        while matches!(self.peek(), Some(Token::Op(op)) if op == "&&") {
            self.next();
            let right = self.parse_unary()?;
            left = Value::Bool(truthy(&left) && truthy(&right));
        }
        Ok(left)
    }

    // unary := '!' unary | comparison
    fn parse_unary(&mut self) -> Result<Value, String> {
        if matches!(self.peek(), Some(Token::Op(op)) if op == "!") {
            self.next();
            let v = self.parse_unary()?;
            return Ok(Value::Bool(!truthy(&v)));
        }
        self.parse_comparison()
    }

    // comparison := primary (op primary)?
    fn parse_comparison(&mut self) -> Result<Value, String> {
        let left = self.parse_primary()?;
        let op = match self.peek() {
            Some(Token::Op(op)) if op != "&&" && op != "||" && op != "!" => self.next().unwrap(),
            _ => return Ok(left),
        };
        let right = self.parse_primary()?;
        let Token::Op(op) = op else { unreachable!() };
        compare(&op, &left, &right)
    }

    // primary := literal | ident | func | '(' expr ')'
    fn parse_primary(&mut self) -> Result<Value, String> {
        match self.next() {
            Some(Token::Str(s)) => Ok(Value::String(s)),
            Some(Token::Num(n)) => {
                if n.fract() == 0.0 && n.abs() < 9e15 {
                    Ok(Value::Number((n as i64).into()))
                } else {
                    Ok(serde_json::Number::from_f64(n).map(Value::Number).unwrap_or(Value::Null))
                }
            }
            Some(Token::Bool(b)) => Ok(Value::Bool(b)),
            Some(Token::LParen) => {
                let v = self.parse_or()?;
                match self.next() {
                    Some(Token::RParen) => Ok(v),
                    _ => Err("expected closing parenthesis".into()),
                }
            }
            Some(Token::Ident(name)) => {
                if matches!(self.peek(), Some(Token::LParen)) {
                    self.next(); // consume '('
                    let mut args = Vec::new();
                    if !matches!(self.peek(), Some(Token::RParen)) {
                        loop {
                            args.push(self.parse_or()?);
                            match self.peek() {
                                Some(Token::Comma) => {
                                    self.next();
                                }
                                _ => break,
                            }
                        }
                    }
                    match self.next() {
                        Some(Token::RParen) => call_function(&name, &args),
                        _ => Err(format!("expected closing parenthesis for function {name}")),
                    }
                } else {
                    Ok(lookup(&self.ctx, &name))
                }
            }
            other => Err(format!("unexpected token in DSL expression: {other:?}")),
        }
    }
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Null => false,
        Value::Number(n) => n.as_f64().unwrap_or(0.0) != 0.0,
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(_) => true,
    }
}

fn compare(op: &str, left: &Value, right: &Value) -> Result<Value, String> {
    let ord = match (left, right) {
        (Value::Number(a), Value::Number(b)) => {
            let (a, b) = (a.as_f64().unwrap_or(0.0), b.as_f64().unwrap_or(0.0));
            a.partial_cmp(&b)
        }
        (Value::String(a), Value::String(b)) => Some(a.cmp(b)),
        (Value::Bool(a), Value::Bool(b)) => Some(a.cmp(b)),
        _ => {
            return Ok(Value::Bool(match op {
                "==" => left == right,
                "!=" => left != right,
                _ => false,
            }))
        }
    };
    let result = match op {
        "==" => ord == Some(std::cmp::Ordering::Equal),
        "!=" => ord != Some(std::cmp::Ordering::Equal),
        "<" => ord == Some(std::cmp::Ordering::Less),
        ">" => ord == Some(std::cmp::Ordering::Greater),
        "<=" => ord != Some(std::cmp::Ordering::Greater),
        ">=" => ord != Some(std::cmp::Ordering::Less),
        other => return Err(format!("unknown comparison operator: {other}")),
    };
    Ok(Value::Bool(result))
}

fn lookup(ctx: &Value, path: &str) -> Value {
    let mut current = ctx;
    for part in path.split('.') {
        match current {
            Value::Object(map) => match map.get(part) {
                Some(v) => current = v,
                None => return Value::Null,
            },
            _ => return Value::Null,
        }
    }
    current.clone()
}

fn as_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn call_function(name: &str, args: &[Value]) -> Result<Value, String> {
    fn arg(args: &[Value], i: usize) -> Result<&Value, String> {
        args.get(i).ok_or_else(|| format!("missing argument {i}"))
    }
    match name {
        "contains" => Ok(Value::Bool(as_string(arg(args, 0)?).contains(&as_string(arg(args, 1)?)))),
        "starts_with" => Ok(Value::Bool(
            as_string(arg(args, 0)?).starts_with(&as_string(arg(args, 1)?)),
        )),
        "ends_with" => Ok(Value::Bool(
            as_string(arg(args, 0)?).ends_with(&as_string(arg(args, 1)?)),
        )),
        "matches" => {
            let a = as_string(arg(args, 0)?);
            let b = as_string(arg(args, 1)?);
            // Accept both celestia conventions: matches(regex, s) and matches(s, regex).
            let a_as_re = regex::Regex::new(&a).ok();
            let b_as_re = regex::Regex::new(&b).ok();
            let hit = a_as_re.map(|re| re.is_match(&b)).unwrap_or(false)
                || b_as_re.map(|re| re.is_match(&a)).unwrap_or(false);
            Ok(Value::Bool(hit))
        }
        "upper" => Ok(Value::String(as_string(arg(args, 0)?).to_uppercase())),
        "lower" => Ok(Value::String(as_string(arg(args, 0)?).to_lowercase())),
        "len" => Ok(Value::Number(
            match arg(args, 0)? {
                Value::String(s) => s.len(),
                Value::Array(a) => a.len(),
                Value::Object(o) => o.len(),
                _ => 0,
            }
            .into(),
        )),
        // words/lines over a string body (used for response stats)
        "words" => Ok(Value::Number(
            as_string(arg(args, 0)?).split_whitespace().count().into(),
        )),
        "lines" => Ok(Value::Number(as_string(arg(args, 0)?).lines().count().into())),
        other => Err(format!("unknown DSL function: {other}")),
    }
}

/// Evaluate a DSL expression against the JSON context; returns whether it is
/// truthy. Errors are reported to the caller (celestia treats eval failure as
/// non-match with a warning).
pub fn eval_bool(expr: &str, ctx: &Value) -> Result<bool, String> {
    let tokens = tokenize(expr)?;
    if tokens.is_empty() {
        return Err("empty DSL expression".into());
    }
    let mut parser = Parser { tokens, pos: 0, ctx: ctx.clone() };
    let value = parser.parse_or()?;
    if parser.pos != parser.tokens.len() {
        return Err(format!(
            "unexpected trailing tokens in DSL expression at position {}",
            parser.pos
        ));
    }
    Ok(truthy(&value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_comparison() {
        let ctx = json!({"url": "https://x.com/a.php", "status_code": 200});
        assert!(eval_bool("status_code == 200", &ctx).unwrap());
        assert!(eval_bool("status_code >= 200 && status_code < 300", &ctx).unwrap());
        assert!(eval_bool("url == 'https://x.com/a.php'", &ctx).unwrap());
    }

    #[test]
    fn test_string_functions() {
        let ctx = json!({"url": "https://example.com/admin"});
        assert!(eval_bool("contains(url, '/admin')", &ctx).unwrap());
        assert!(eval_bool("starts_with(url, 'https://')", &ctx).unwrap());
        assert!(eval_bool("ends_with(url, 'admin')", &ctx).unwrap());
        assert!(eval_bool("matches(url, '.*example.*')", &ctx).unwrap());
        assert!(eval_bool("lower(url) == 'https://example.com/admin'", &ctx).unwrap());
    }

    #[test]
    fn test_boolean_ops_and_negation() {
        let ctx = json!({"a": 1, "b": 2});
        assert!(eval_bool("a == 1 || b == 3", &ctx).unwrap());
        assert!(eval_bool("a == 1 && b == 2", &ctx).unwrap());
        assert!(eval_bool("!(a == 2)", &ctx).unwrap());
    }

    #[test]
    fn test_nested_paths_and_len_words_lines() {
        let ctx = json!({"response": {"body": "one two\nthree"}, "nested": {"x": [1,2,3]}});
        assert!(eval_bool("len(nested.x) == 3", &ctx).unwrap());
        assert!(eval_bool("words(response.body) == 3", &ctx).unwrap());
        assert!(eval_bool("lines(response.body) == 2", &ctx).unwrap());
    }

    #[test]
    fn test_errors() {
        let ctx = json!({"a": 1});
        assert!(eval_bool("", &ctx).is_err());
        assert!(eval_bool("a == ", &ctx).is_err());
        assert!(eval_bool("nope_func(a)", &ctx).is_err());
    }

    #[test]
    fn test_missing_field_is_null() {
        let ctx = json!({});
        assert!(eval_bool("missing != 'x'", &ctx).unwrap());
        assert!(!eval_bool("contains(missing, 'x')", &ctx).unwrap());
    }
}
