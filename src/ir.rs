//! The Whetstone intermediate representation and its parser.
//!
//! A program is a flat sequence of three-address instructions over
//! virtual registers. Two kinds of instruction exist: an assignment
//! that computes a value into a register, and a print that observes
//! a value. Print is the only instruction with a side effect, which
//! is what makes dead-code elimination decidable: anything that is
//! not a print and whose result register is never read again can go.

use std::fmt;

/// A binary operator supported by the IR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl BinOp {
    fn as_str(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
        }
    }

    fn from_str(s: &str) -> Option<BinOp> {
        match s {
            "+" => Some(BinOp::Add),
            "-" => Some(BinOp::Sub),
            "*" => Some(BinOp::Mul),
            "/" => Some(BinOp::Div),
            "==" => Some(BinOp::Eq),
            "!=" => Some(BinOp::Ne),
            "<" => Some(BinOp::Lt),
            "<=" => Some(BinOp::Le),
            ">" => Some(BinOp::Gt),
            ">=" => Some(BinOp::Ge),
            _ => None,
        }
    }

    /// Apply the operator to two constant integers, folding it away.
    /// Returns `None` for a division by zero, which is left unfolded
    /// so it can fail at runtime instead of at compile time.
    pub fn apply(self, a: i64, b: i64) -> Option<i64> {
        match self {
            BinOp::Add => Some(a.wrapping_add(b)),
            BinOp::Sub => Some(a.wrapping_sub(b)),
            BinOp::Mul => Some(a.wrapping_mul(b)),
            BinOp::Div => {
                if b == 0 {
                    None
                } else {
                    Some(a.wrapping_div(b))
                }
            }
            BinOp::Eq => Some((a == b) as i64),
            BinOp::Ne => Some((a != b) as i64),
            BinOp::Lt => Some((a < b) as i64),
            BinOp::Le => Some((a <= b) as i64),
            BinOp::Gt => Some((a > b) as i64),
            BinOp::Ge => Some((a >= b) as i64),
        }
    }
}

impl fmt::Display for BinOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A value used as an operand: either a literal integer or a
/// reference to a virtual register.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Const(i64),
    Reg(String),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Const(n) => write!(f, "{n}"),
            Value::Reg(r) => write!(f, "{r}"),
        }
    }
}

/// The right-hand side of an assignment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    /// A bare value, e.g. `t2 = t1` or `t2 = 5`.
    Copy(Value),
    /// A binary operation, e.g. `t2 = t1 * x`.
    Bin(Value, BinOp, Value),
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Copy(v) => write!(f, "{v}"),
            Expr::Bin(a, op, b) => write!(f, "{a} {op} {b}"),
        }
    }
}

/// One instruction in the program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Instr {
    /// `dst = expr`
    Assign { dst: String, expr: Expr },
    /// `print value`, the only instruction with an observable effect.
    Print(Value),
}

impl Instr {
    /// The register this instruction defines, if any.
    pub fn def(&self) -> Option<&str> {
        match self {
            Instr::Assign { dst, .. } => Some(dst.as_str()),
            Instr::Print(_) => None,
        }
    }

    /// The registers this instruction reads.
    pub fn uses(&self) -> Vec<&str> {
        fn val_reg(v: &Value) -> Option<&str> {
            match v {
                Value::Reg(r) => Some(r.as_str()),
                Value::Const(_) => None,
            }
        }
        match self {
            Instr::Assign { expr, .. } => match expr {
                Expr::Copy(v) => val_reg(v).into_iter().collect(),
                Expr::Bin(a, _, b) => [val_reg(a), val_reg(b)].into_iter().flatten().collect(),
            },
            Instr::Print(v) => val_reg(v).into_iter().collect(),
        }
    }

    /// True if this instruction has a side effect and can never be
    /// dropped by dead-code elimination.
    pub fn has_side_effect(&self) -> bool {
        matches!(self, Instr::Print(_))
    }
}

impl fmt::Display for Instr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Instr::Assign { dst, expr } => write!(f, "{dst} = {expr}"),
            Instr::Print(v) => write!(f, "print {v}"),
        }
    }
}

/// A whole program: an ordered list of instructions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Program {
    pub instrs: Vec<Instr>,
}

impl fmt::Display for Program {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, instr) in self.instrs.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "{instr}")?;
        }
        Ok(())
    }
}

/// A parse error, with the 1-based line number and a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn parse_value(tok: &str, line: usize) -> Result<Value, ParseError> {
    if let Ok(n) = tok.parse::<i64>() {
        return Ok(Value::Const(n));
    }
    if is_ident(tok) {
        return Ok(Value::Reg(tok.to_string()));
    }
    Err(ParseError {
        line,
        message: format!("expected a number or a register name, found `{tok}`"),
    })
}

/// Parse Whetstone IR text into a `Program`.
///
/// Grammar, one instruction per line, blank lines and lines starting
/// with `#` are ignored:
/// ```text
/// dst = value
/// dst = value op value
/// print value
/// ```
pub fn parse(source: &str) -> Result<Program, ParseError> {
    let mut instrs = Vec::new();
    for (idx, raw_line) in source.lines().enumerate() {
        let line = idx + 1;
        let text = match raw_line.find('#') {
            Some(pos) => &raw_line[..pos],
            None => raw_line,
        };
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        let tokens: Vec<&str> = text.split_whitespace().collect();

        if tokens[0] == "print" {
            if tokens.len() != 2 {
                return Err(ParseError {
                    line,
                    message: format!("`print` takes exactly one value, found {} tokens", tokens.len() - 1),
                });
            }
            let v = parse_value(tokens[1], line)?;
            instrs.push(Instr::Print(v));
            continue;
        }

        // Assignment: dst = value [op value]
        if tokens.len() < 3 || tokens[1] != "=" {
            return Err(ParseError {
                line,
                message: "expected `dst = value`, `dst = value op value`, or `print value`".to_string(),
            });
        }
        let dst = tokens[0];
        if !is_ident(dst) {
            return Err(ParseError {
                line,
                message: format!("`{dst}` is not a valid register name"),
            });
        }
        let expr = match tokens.len() {
            3 => Expr::Copy(parse_value(tokens[2], line)?),
            5 => {
                let a = parse_value(tokens[2], line)?;
                let op = BinOp::from_str(tokens[3]).ok_or_else(|| ParseError {
                    line,
                    message: format!("unknown operator `{}`", tokens[3]),
                })?;
                let b = parse_value(tokens[4], line)?;
                Expr::Bin(a, op, b)
            }
            n => {
                return Err(ParseError {
                    line,
                    message: format!("expected `dst = value` or `dst = value op value`, found {n} tokens"),
                })
            }
        };
        instrs.push(Instr::Assign {
            dst: dst.to_string(),
            expr,
        });
    }
    Ok(Program { instrs })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_simple_program() {
        let src = "t1 = 2 + 3\nt2 = t1 * x\nprint t2\n";
        let prog = parse(src).unwrap();
        assert_eq!(prog.instrs.len(), 3);
        assert_eq!(
            prog.instrs[0],
            Instr::Assign {
                dst: "t1".into(),
                expr: Expr::Bin(Value::Const(2), BinOp::Add, Value::Const(3))
            }
        );
    }

    #[test]
    fn rejects_bad_ir_with_line_number() {
        let src = "t1 = 2 +\n";
        let err = parse(src).unwrap_err();
        assert_eq!(err.line, 1);
    }

    #[test]
    fn skips_blank_lines_and_comments() {
        let src = "# a comment\n\nt1 = 5\n";
        let prog = parse(src).unwrap();
        assert_eq!(prog.instrs.len(), 1);
    }
}
