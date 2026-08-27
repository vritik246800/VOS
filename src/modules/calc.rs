//! A small self-contained expression evaluator for the popup Calculator.
//!
//! Implements a shunting-yard parser (no external dependency) supporting:
//!   * binary operators: `+ - * / % ^`
//!   * parentheses
//!   * unary minus (e.g. `-3`, `2 * -4`)
//!   * functions: `sqrt sin cos log`
//!   * decimal, hex (`0x`) and binary (`0b`) integer literals
//!
//! The evaluator never panics: malformed input or runtime errors (such as
//! divide-by-zero) are returned as `Err(String)` carrying a UI-friendly
//! message.

#![allow(dead_code)]

/// Token kinds produced by the lexer.
#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Op(char),
    LParen,
    RParen,
    Func(String),
    /// Unary minus — distinct from binary `-`.
    Neg,
}

/// Evaluate an arithmetic expression, returning the numeric result or a
/// human-readable error message suitable for display in the UI.
pub fn eval(expr: &str) -> Result<f64, String> {
    let tokens = tokenize(expr)?;
    if tokens.is_empty() {
        return Err("empty expression".to_string());
    }
    let rpn = to_rpn(tokens)?;
    eval_rpn(rpn)
}

/// Lexer: turn the raw string into a token stream, resolving unary vs binary
/// minus on the fly.
fn tokenize(expr: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = expr.chars().collect();
    let mut tokens: Vec<Token> = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        if c.is_whitespace() {
            i += 1;
            continue;
        }

        // ── Numbers (decimal, 0x hex, 0b binary) ─────────────────────────────
        if c.is_ascii_digit() || (c == '.' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit())
        {
            // hex / binary prefixes
            if c == '0' && i + 1 < chars.len() && (chars[i + 1] == 'x' || chars[i + 1] == 'X') {
                let start = i + 2;
                let mut j = start;
                while j < chars.len() && chars[j].is_ascii_hexdigit() {
                    j += 1;
                }
                if j == start {
                    return Err("malformed hex literal".to_string());
                }
                let s: String = chars[start..j].iter().collect();
                let v =
                    i64::from_str_radix(&s, 16).map_err(|_| "invalid hex literal".to_string())?;
                tokens.push(Token::Number(v as f64));
                i = j;
                continue;
            }
            if c == '0' && i + 1 < chars.len() && (chars[i + 1] == 'b' || chars[i + 1] == 'B') {
                let start = i + 2;
                let mut j = start;
                while j < chars.len() && (chars[j] == '0' || chars[j] == '1') {
                    j += 1;
                }
                if j == start {
                    return Err("malformed binary literal".to_string());
                }
                let s: String = chars[start..j].iter().collect();
                let v =
                    i64::from_str_radix(&s, 2).map_err(|_| "invalid binary literal".to_string())?;
                tokens.push(Token::Number(v as f64));
                i = j;
                continue;
            }

            // decimal (integer or float)
            let start = i;
            let mut seen_dot = false;
            while i < chars.len() && (chars[i].is_ascii_digit() || (chars[i] == '.' && !seen_dot)) {
                if chars[i] == '.' {
                    seen_dot = true;
                }
                i += 1;
            }
            let s: String = chars[start..i].iter().collect();
            let v: f64 = s.parse().map_err(|_| format!("invalid number '{s}'"))?;
            tokens.push(Token::Number(v));
            continue;
        }

        // ── Identifiers / functions ──────────────────────────────────────────
        if c.is_ascii_alphabetic() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_alphabetic() {
                i += 1;
            }
            let name: String = chars[start..i].iter().collect();
            match name.as_str() {
                "sqrt" | "sin" | "cos" | "log" => tokens.push(Token::Func(name)),
                other => return Err(format!("unknown function '{other}'")),
            }
            continue;
        }

        // ── Operators / parentheses ──────────────────────────────────────────
        match c {
            '(' => tokens.push(Token::LParen),
            ')' => tokens.push(Token::RParen),
            '+' | '*' | '/' | '%' | '^' => tokens.push(Token::Op(c)),
            '-' => {
                // Unary if at start, or following another op / '(' / a function.
                let unary = matches!(
                    tokens.last(),
                    None | Some(Token::Op(_))
                        | Some(Token::LParen)
                        | Some(Token::Neg)
                        | Some(Token::Func(_))
                );
                if unary {
                    tokens.push(Token::Neg);
                } else {
                    tokens.push(Token::Op('-'));
                }
            }
            _ => return Err(format!("unexpected character '{c}'")),
        }
        i += 1;
    }

    Ok(tokens)
}

/// Operator precedence (higher binds tighter). Unary minus sits above the
/// binary operators but below `^`.
fn precedence(tok: &Token) -> u8 {
    match tok {
        Token::Op('+') | Token::Op('-') => 1,
        Token::Op('*') | Token::Op('/') | Token::Op('%') => 2,
        Token::Neg => 3,
        Token::Op('^') => 4,
        Token::Func(_) => 5,
        _ => 0,
    }
}

/// `^` is right-associative; everything else is left-associative.
fn right_assoc(tok: &Token) -> bool {
    matches!(tok, Token::Op('^') | Token::Neg)
}

/// Shunting-yard: convert an infix token stream into Reverse Polish Notation.
fn to_rpn(tokens: Vec<Token>) -> Result<Vec<Token>, String> {
    let mut output: Vec<Token> = Vec::new();
    let mut ops: Vec<Token> = Vec::new();

    for tok in tokens {
        match tok {
            Token::Number(_) => output.push(tok),
            Token::Func(_) => ops.push(tok),
            Token::Neg => {
                // Unary: pop only strictly-higher precedence (i.e. `^`).
                while let Some(top) = ops.last() {
                    if matches!(top, Token::LParen) {
                        break;
                    }
                    if precedence(top) > precedence(&tok) {
                        output.push(ops.pop().unwrap());
                    } else {
                        break;
                    }
                }
                ops.push(tok);
            }
            Token::Op(_) => {
                while let Some(top) = ops.last() {
                    if matches!(top, Token::LParen) {
                        break;
                    }
                    let pt = precedence(top);
                    let pc = precedence(&tok);
                    if pt > pc || (pt == pc && !right_assoc(&tok)) {
                        output.push(ops.pop().unwrap());
                    } else {
                        break;
                    }
                }
                ops.push(tok);
            }
            Token::LParen => ops.push(tok),
            Token::RParen => {
                let mut found = false;
                while let Some(top) = ops.pop() {
                    if matches!(top, Token::LParen) {
                        found = true;
                        break;
                    }
                    output.push(top);
                }
                if !found {
                    return Err("mismatched parentheses".to_string());
                }
                // A function immediately preceding the '(' applies to its result.
                if let Some(Token::Func(_)) = ops.last() {
                    output.push(ops.pop().unwrap());
                }
            }
        }
    }

    while let Some(top) = ops.pop() {
        if matches!(top, Token::LParen | Token::RParen) {
            return Err("mismatched parentheses".to_string());
        }
        output.push(top);
    }

    Ok(output)
}

/// Evaluate a Reverse Polish Notation token stream.
fn eval_rpn(rpn: Vec<Token>) -> Result<f64, String> {
    let mut stack: Vec<f64> = Vec::new();

    for tok in rpn {
        match tok {
            Token::Number(n) => stack.push(n),
            Token::Neg => {
                let a = stack
                    .pop()
                    .ok_or_else(|| "malformed expression".to_string())?;
                stack.push(-a);
            }
            Token::Func(name) => {
                let a = stack
                    .pop()
                    .ok_or_else(|| "malformed expression".to_string())?;
                let v = match name.as_str() {
                    "sqrt" => {
                        if a < 0.0 {
                            return Err("sqrt of negative number".to_string());
                        }
                        a.sqrt()
                    }
                    "sin" => a.sin(),
                    "cos" => a.cos(),
                    "log" => {
                        if a <= 0.0 {
                            return Err("log of non-positive number".to_string());
                        }
                        a.ln()
                    }
                    _ => return Err(format!("unknown function '{name}'")),
                };
                stack.push(v);
            }
            Token::Op(op) => {
                let b = stack
                    .pop()
                    .ok_or_else(|| "malformed expression".to_string())?;
                let a = stack
                    .pop()
                    .ok_or_else(|| "malformed expression".to_string())?;
                let v = match op {
                    '+' => a + b,
                    '-' => a - b,
                    '*' => a * b,
                    '/' => {
                        if b == 0.0 {
                            return Err("division by zero".to_string());
                        }
                        a / b
                    }
                    '%' => {
                        if b == 0.0 {
                            return Err("modulo by zero".to_string());
                        }
                        a % b
                    }
                    '^' => a.powf(b),
                    _ => return Err(format!("unknown operator '{op}'")),
                };
                stack.push(v);
            }
            Token::LParen | Token::RParen => {
                return Err("malformed expression".to_string());
            }
        }
    }

    match stack.len() {
        1 => Ok(stack[0]),
        0 => Err("empty expression".to_string()),
        _ => Err("malformed expression".to_string()),
    }
}

// ── Calculator state ────────────────────────────────────────────────────────────

/// Calculator popup state: live input, the current evaluation result, a
/// committed history of `(expression, value)` pairs, and bottom-sheet
/// slide animation state.
pub struct Calculator {
    pub input: String,
    pub result: Result<f64, String>,
    pub history: Vec<(String, f64)>,
    /// Current animation position: 0 = fully hidden, 100 = fully open.
    pub anim_pct: u16,
    /// Animation target: 0 = closing, 100 = opening.
    pub anim_target: u16,
}

impl Calculator {
    pub fn new() -> Self {
        Self {
            input: String::new(),
            result: Err(String::new()),
            history: Vec::new(),
            anim_pct: 0,
            anim_target: 0,
        }
    }

    /// Recompute `result` from the current `input`. An empty input yields a
    /// blank (non-displayed) error rather than a noisy message.
    pub fn evaluate_live(&mut self) {
        if self.input.trim().is_empty() {
            self.result = Err(String::new());
        } else {
            self.result = eval(&self.input);
        }
    }

    pub fn push_char(&mut self, c: char) {
        self.input.push(c);
        self.evaluate_live();
    }

    pub fn backspace(&mut self) {
        self.input.pop();
        self.evaluate_live();
    }

    pub fn clear(&mut self) {
        self.input.clear();
        self.evaluate_live();
    }

    /// Commit the current expression to history on Enter. Returns the computed
    /// value when the expression is valid (so the caller may copy it to the
    /// clipboard), or `None` otherwise.
    pub fn commit(&mut self) -> Option<f64> {
        self.evaluate_live();
        if let Ok(v) = self.result {
            self.history.push((self.input.clone(), v));
            self.input.clear();
            self.result = Err(String::new());
            Some(v)
        } else {
            None
        }
    }

    /// Step `anim_pct` toward `anim_target` by 8 per tick.
    pub fn tick_anim(&mut self) {
        if self.anim_pct < self.anim_target {
            self.anim_pct = (self.anim_pct + 8).min(self.anim_target);
        } else if self.anim_pct > self.anim_target {
            self.anim_pct = self.anim_pct.saturating_sub(8).max(self.anim_target);
        }
    }

    /// Returns true while the panel is visible (either animating in or still open).
    pub fn is_visible(&self) -> bool {
        self.anim_pct > 0 || self.anim_target > 0
    }
}

impl Default for Calculator {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(expr: &str) -> f64 {
        eval(expr).unwrap_or_else(|e| panic!("expected ok for '{expr}', got error: {e}"))
    }

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn test_basic_arithmetic() {
        assert!(approx(ok("1 + 2"), 3.0));
        assert!(approx(ok("10 - 4"), 6.0));
        assert!(approx(ok("6 * 7"), 42.0));
        assert!(approx(ok("20 / 5"), 4.0));
        assert!(approx(ok("10 % 3"), 1.0));
    }

    #[test]
    fn test_precedence() {
        assert!(approx(ok("2 + 3 * 4"), 14.0));
        assert!(approx(ok("2 * 3 + 4"), 10.0));
        assert!(approx(ok("10 - 2 - 3"), 5.0)); // left associative
        assert!(approx(ok("2 + 10 / 2"), 7.0));
    }

    #[test]
    fn test_power_right_assoc() {
        assert!(approx(ok("2 ^ 3"), 8.0));
        assert!(approx(ok("2 ^ 3 ^ 2"), 512.0)); // 2^(3^2)
        assert!(approx(ok("2 * 2 ^ 3"), 16.0)); // ^ before *
    }

    #[test]
    fn test_parentheses() {
        assert!(approx(ok("(2 + 3) * 4"), 20.0));
        assert!(approx(ok("2 * (3 + (4 - 1))"), 12.0));
        assert!(approx(ok("((1 + 1))"), 2.0));
    }

    #[test]
    fn test_unary_minus() {
        assert!(approx(ok("-5"), -5.0));
        assert!(approx(ok("-5 + 3"), -2.0));
        assert!(approx(ok("2 * -4"), -8.0));
        assert!(approx(ok("-(2 + 3)"), -5.0));
        assert!(approx(ok("3 - -2"), 5.0));
        assert!(approx(ok("-2 ^ 2"), -4.0)); // unary below ^: -(2^2)
    }

    #[test]
    fn test_functions() {
        assert!(approx(ok("sqrt(16)"), 4.0));
        assert!(approx(ok("sqrt(9) + 1"), 4.0));
        assert!(approx(ok("log(1)"), 0.0));
        assert!(approx(ok("sin(0)"), 0.0));
        assert!(approx(ok("cos(0)"), 1.0));
        assert!(approx(ok("sqrt(4) * 3"), 6.0));
    }

    #[test]
    fn test_hex_literals() {
        assert!(approx(ok("0xff"), 255.0));
        assert!(approx(ok("0x10"), 16.0));
        assert!(approx(ok("0xFF + 1"), 256.0));
        assert!(approx(ok("0x0a * 2"), 20.0));
    }

    #[test]
    fn test_bin_literals() {
        assert!(approx(ok("0b1010"), 10.0));
        assert!(approx(ok("0b1111"), 15.0));
        assert!(approx(ok("0b10 + 0b01"), 3.0));
    }

    #[test]
    fn test_float() {
        assert!(approx(ok("1.5 + 2.5"), 4.0));
        assert!(approx(ok("0.1 * 10"), 1.0));
        assert!(approx(ok(".5 + .5"), 1.0));
    }

    #[test]
    fn test_divide_by_zero() {
        assert!(eval("1 / 0").is_err());
        assert!(eval("5 % 0").is_err());
    }

    #[test]
    fn test_malformed_input() {
        assert!(eval("").is_err());
        assert!(eval("1 +").is_err());
        assert!(eval("* 2").is_err());
        assert!(eval("(1 + 2").is_err());
        assert!(eval("1 + 2)").is_err());
        assert!(eval("1 2").is_err());
        assert!(eval("foo(2)").is_err());
        assert!(eval("0x").is_err());
        assert!(eval("0b").is_err());
        assert!(eval("@").is_err());
    }

    #[test]
    fn test_never_panics_on_garbage() {
        for s in ["", "()", ")(", "+++", "sqrt", "sqrt()", "1//2", "..", "0xG"] {
            let _ = eval(s); // must not panic
        }
    }

    #[test]
    fn test_calculator_commit() {
        let mut calc = Calculator::new();
        for c in "2+3".chars() {
            calc.push_char(c);
        }
        assert_eq!(calc.result, Ok(5.0));
        let committed = calc.commit();
        assert_eq!(committed, Some(5.0));
        assert_eq!(calc.history.len(), 1);
        assert_eq!(calc.history[0], ("2+3".to_string(), 5.0));
        assert!(calc.input.is_empty());
    }

    #[test]
    fn test_calculator_commit_invalid() {
        let mut calc = Calculator::new();
        for c in "1+".chars() {
            calc.push_char(c);
        }
        assert!(calc.result.is_err());
        assert_eq!(calc.commit(), None);
        assert!(calc.history.is_empty());
    }

    #[test]
    fn test_calculator_backspace() {
        let mut calc = Calculator::new();
        for c in "12".chars() {
            calc.push_char(c);
        }
        calc.backspace();
        assert_eq!(calc.input, "1");
        assert_eq!(calc.result, Ok(1.0));
    }
}
