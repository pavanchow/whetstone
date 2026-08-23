//! Optimization passes over the Whetstone IR.
//!
//! Every pass is a pure function from `Program` to `Program` plus a
//! list of human-readable notes describing what it changed. The IR
//! has no branches or jumps, so liveness and constant state can be
//! computed with a single linear scan, no fixed-point dataflow
//! analysis is needed inside a pass. The fixed point that does
//! matter is across passes: folding can expose new dead code, and
//! dead-code elimination can expose new folding opportunities, so
//! `optimize` reruns the whole pipeline until nothing changes.

use crate::ir::{BinOp, Expr, Instr, Program, Value};
use std::collections::HashMap;

/// Hard cap on pipeline iterations, so a pathological input can
/// never spin forever even if a bug made two passes fight each
/// other. A real fixed point is always found in a handful of passes
/// over straight-line code far below this cap.
pub const MAX_ITERATIONS: usize = 100;

/// The result of a single pass, including a before/after snapshot
/// and notes for `--show-passes`.
pub struct PassReport {
    pub name: &'static str,
    pub iteration: usize,
    pub before: Program,
    pub after: Program,
    pub notes: Vec<String>,
}

impl PassReport {
    fn changed(&self) -> bool {
        self.before != self.after
    }
}

/// The result of running the full pipeline to a fixed point.
pub struct OptimizeResult {
    pub original: Program,
    pub final_program: Program,
    pub reports: Vec<PassReport>,
    pub iterations: usize,
    pub hit_cap: bool,
}

impl OptimizeResult {
    /// Notes from every pass that actually changed something,
    /// in the order the passes ran.
    pub fn all_notes(&self) -> Vec<String> {
        self.reports
            .iter()
            .filter(|r| r.changed())
            .flat_map(|r| r.notes.iter().cloned())
            .collect()
    }
}

/// Run the full pipeline (fold, propagate, simplify, eliminate dead
/// code) repeatedly until a full round makes no change, or the
/// iteration cap is hit.
pub fn optimize(prog: &Program) -> OptimizeResult {
    let mut current = prog.clone();
    let mut reports = Vec::new();
    let mut iterations = 0;
    let mut hit_cap = true;

    for iter in 1..=MAX_ITERATIONS {
        iterations = iter;
        let mut round_changed = false;

        for (name, pass) in PASSES {
            let before = current.clone();
            let (after, notes) = pass(&before);
            let changed = after != before;
            round_changed |= changed;
            reports.push(PassReport {
                name,
                iteration: iter,
                before,
                after: after.clone(),
                notes,
            });
            current = after;
        }

        if !round_changed {
            hit_cap = false;
            break;
        }
    }

    OptimizeResult {
        original: prog.clone(),
        final_program: current,
        reports,
        iterations,
        hit_cap,
    }
}

type Pass = fn(&Program) -> (Program, Vec<String>);

const PASSES: &[(&str, Pass)] = &[
    ("constant folding", constant_fold),
    ("constant propagation", constant_propagate),
    ("algebraic simplification", algebraic_simplify),
    ("dead code elimination", dead_code_eliminate),
];

/// Fold a binary operation over two literal constants into a single
/// literal. `t1 = 2 + 3` becomes `t1 = 5`. A fold that would divide
/// by zero is left alone, so the program can still fail at runtime
/// instead of silently changing meaning.
pub fn constant_fold(prog: &Program) -> (Program, Vec<String>) {
    let mut notes = Vec::new();
    let mut instrs = Vec::with_capacity(prog.instrs.len());
    for instr in &prog.instrs {
        match instr {
            Instr::Assign {
                dst,
                expr: Expr::Bin(Value::Const(a), op, Value::Const(b)),
            } => match op.apply(*a, *b) {
                Some(result) => {
                    let folded = Instr::Assign {
                        dst: dst.clone(),
                        expr: Expr::Copy(Value::Const(result)),
                    };
                    notes.push(format!("constant folding: `{instr}` -> `{folded}`"));
                    instrs.push(folded);
                }
                None => instrs.push(instr.clone()),
            },
            other => instrs.push(other.clone()),
        }
    }
    (Program { instrs }, notes)
}

/// Replace a register operand with the constant it is known to hold
/// at that point in the program. Because the IR is straight-line
/// with no branches, a single forward scan tracking "this register
/// currently holds constant N" is exact: a later reassignment to a
/// non-constant simply drops the register from the known-constant
/// map for every instruction after it.
pub fn constant_propagate(prog: &Program) -> (Program, Vec<String>) {
    let mut notes = Vec::new();
    let mut known: HashMap<String, i64> = HashMap::new();
    let mut instrs = Vec::with_capacity(prog.instrs.len());

    let sub = |v: &Value, known: &HashMap<String, i64>| -> Option<Value> {
        if let Value::Reg(r) = v {
            known.get(r).map(|n| Value::Const(*n))
        } else {
            None
        }
    };

    for instr in &prog.instrs {
        let new_instr = match instr {
            Instr::Assign { dst, expr } => {
                let new_expr = match expr {
                    Expr::Copy(v) => match sub(v, &known) {
                        Some(nv) => Expr::Copy(nv),
                        None => expr.clone(),
                    },
                    Expr::Bin(a, op, b) => {
                        let na = sub(a, &known).unwrap_or_else(|| a.clone());
                        let nb = sub(b, &known).unwrap_or_else(|| b.clone());
                        Expr::Bin(na, *op, nb)
                    }
                };
                Instr::Assign {
                    dst: dst.clone(),
                    expr: new_expr,
                }
            }
            Instr::Print(v) => match sub(v, &known) {
                Some(nv) => Instr::Print(nv),
                None => instr.clone(),
            },
        };

        if new_instr != *instr {
            notes.push(format!("constant propagation: `{instr}` -> `{new_instr}`"));
        }

        // Update the known-constant map for what comes after.
        if let Instr::Assign { dst, expr } = &new_instr {
            match expr {
                Expr::Copy(Value::Const(n)) => {
                    known.insert(dst.clone(), *n);
                }
                _ => {
                    known.remove(dst);
                }
            }
        }

        instrs.push(new_instr);
    }
    (Program { instrs }, notes)
}

/// Rewrite identities that do not need arithmetic at all:
/// `x + 0`, `0 + x`, `x - 0` -> `x`
/// `x * 1`, `1 * x`, `x / 1` -> `x`
/// `x * 0`, `0 * x` -> `0`
pub fn algebraic_simplify(prog: &Program) -> (Program, Vec<String>) {
    let mut notes = Vec::new();
    let mut instrs = Vec::with_capacity(prog.instrs.len());

    for instr in &prog.instrs {
        if let Instr::Assign {
            dst,
            expr: Expr::Bin(a, op, b),
        } = instr
        {
            let simplified = match (a, op, b) {
                (v, BinOp::Add, Value::Const(0)) => Some(v.clone()),
                (Value::Const(0), BinOp::Add, v) => Some(v.clone()),
                (v, BinOp::Sub, Value::Const(0)) => Some(v.clone()),
                (v, BinOp::Mul, Value::Const(1)) => Some(v.clone()),
                (Value::Const(1), BinOp::Mul, v) => Some(v.clone()),
                (v, BinOp::Div, Value::Const(1)) => Some(v.clone()),
                (_, BinOp::Mul, Value::Const(0)) => Some(Value::Const(0)),
                (Value::Const(0), BinOp::Mul, _) => Some(Value::Const(0)),
                _ => None,
            };
            if let Some(v) = simplified {
                let new_instr = Instr::Assign {
                    dst: dst.clone(),
                    expr: Expr::Copy(v),
                };
                notes.push(format!("algebraic simplification: `{instr}` -> `{new_instr}`"));
                instrs.push(new_instr);
                continue;
            }
        }
        instrs.push(instr.clone());
    }
    (Program { instrs }, notes)
}

/// Drop any assignment whose result is never used again and that
/// has no side effect. A single backward scan is enough: walk from
/// the last instruction to the first, tracking which registers are
/// still needed by something kept so far.
pub fn dead_code_eliminate(prog: &Program) -> (Program, Vec<String>) {
    let mut notes = Vec::new();
    let mut live: HashMap<String, ()> = HashMap::new();
    let mut kept_rev = Vec::with_capacity(prog.instrs.len());

    for instr in prog.instrs.iter().rev() {
        let keep = match instr {
            Instr::Print(_) => true,
            Instr::Assign { dst, .. } => instr.has_side_effect() || live.remove(dst).is_some(),
        };

        if !keep {
            notes.push(format!("dead code elimination: removed `{instr}` (result never used)"));
            continue;
        }

        for used in instr.uses() {
            live.insert(used.to_string(), ());
        }
        kept_rev.push(instr.clone());
    }

    kept_rev.reverse();
    // Notes were pushed while scanning backward; present them in
    // forward program order so `--show-passes` output reads top to
    // bottom, matching every other pass.
    notes.reverse();
    (Program { instrs: kept_rev }, notes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::parse;

    #[test]
    fn folds_a_literal_chain() {
        let prog = parse("t1 = 2 + 3\nt2 = t1 * 4\nprint t2\n").unwrap();
        let result = optimize(&prog);
        // Folds to a single constant, then t1 and t2 are dead once
        // their value has been propagated all the way to the print.
        assert_eq!(result.final_program.to_string(), "print 20");
    }

    #[test]
    fn propagates_then_folds_a_dependent_chain() {
        let prog = parse("t1 = 5\nt2 = t1\nt3 = t2 + 1\nprint t3\n").unwrap();
        let result = optimize(&prog);
        // t1 is dead once everything downstream is folded to a literal.
        assert_eq!(result.final_program.to_string(), "print 6");
    }

    #[test]
    fn simplifies_identity_multiply_and_add() {
        // Constant propagation only tracks registers known to hold a
        // constant, x is a free variable, so the register-to-register
        // copies stay, but each identity rewrites to a plain copy.
        let prog = parse("t1 = x * 1\nt2 = t1 + 0\nprint t2\n").unwrap();
        let result = optimize(&prog);
        assert_eq!(result.final_program.to_string(), "t1 = x\nt2 = t1\nprint t2");
    }

    #[test]
    fn simplifies_multiply_by_zero() {
        let prog = parse("t1 = x * 0\nprint t1\n").unwrap();
        let result = optimize(&prog);
        assert_eq!(result.final_program.to_string(), "print 0");
    }

    #[test]
    fn removes_unused_pure_instruction() {
        let prog = parse("t1 = x + 1\nt2 = 5\nprint t2\n").unwrap();
        let result = optimize(&prog);
        assert_eq!(result.final_program.to_string(), "print 5");
    }

    #[test]
    fn keeps_a_value_that_is_used() {
        let prog = parse("t1 = x + 1\nprint t1\n").unwrap();
        let result = optimize(&prog);
        assert_eq!(result.final_program.to_string(), "t1 = x + 1\nprint t1");
    }

    #[test]
    fn terminates_within_the_iteration_cap() {
        let mut src = String::new();
        for i in 0..200 {
            src.push_str(&format!("t{i} = {i} + 1\n"));
        }
        src.push_str("print t0\n");
        let prog = parse(&src).unwrap();
        let result = optimize(&prog);
        assert!(result.iterations <= MAX_ITERATIONS);
        assert!(!result.hit_cap, "pipeline should reach a real fixed point, not the cap");
    }
}
