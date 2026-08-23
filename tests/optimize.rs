use whetstone::ir::parse;
use whetstone::passes::{optimize, MAX_ITERATIONS};

#[test]
fn constant_folding_reduces_a_literal_chain_to_one_constant() {
    let prog = parse("t1 = 2 + 3\nt2 = t1 * 4\nt3 = t2 - 1\nprint t3\n").unwrap();
    let result = optimize(&prog);
    // (2 + 3) * 4 - 1 = 19, and t1/t2 die once t3 is a literal.
    assert_eq!(result.final_program.to_string(), "print 19");
}

#[test]
fn constant_propagation_plus_folding_collapses_a_dependent_chain() {
    let prog = parse("a = 10\nb = a\nc = b + 5\nd = c\nprint d\n").unwrap();
    let result = optimize(&prog);
    assert_eq!(result.final_program.to_string(), "print 15");
}

#[test]
fn x_times_one_and_x_plus_zero_simplify_to_x() {
    // Each identity collapses to a plain copy of x. Constant
    // propagation does not chase non-constant register copies, so
    // the two assignments remain, each now trivial.
    let prog = parse("t1 = x * 1\nt2 = t1 + 0\nprint t2\n").unwrap();
    let result = optimize(&prog);
    assert_eq!(result.final_program.to_string(), "t1 = x\nt2 = t1\nprint t2");
}

#[test]
fn x_times_one_directly_used_simplifies_in_place() {
    let prog = parse("t1 = x * 1\nprint t1\n").unwrap();
    let result = optimize(&prog);
    assert_eq!(result.final_program.to_string(), "t1 = x\nprint t1");
}

#[test]
fn x_times_zero_becomes_zero() {
    let prog = parse("t1 = x * 0\nprint t1\n").unwrap();
    let result = optimize(&prog);
    assert_eq!(result.final_program.to_string(), "print 0");
}

#[test]
fn unused_pure_instruction_is_removed() {
    let prog = parse("dead = x + 99\nkept = 3\nprint kept\n").unwrap();
    let result = optimize(&prog);
    let out = result.final_program.to_string();
    assert_eq!(out, "print 3");
    assert!(!out.contains("dead"));
}

#[test]
fn a_used_value_is_kept() {
    let prog = parse("t1 = x + 1\nprint t1\n").unwrap();
    let result = optimize(&prog);
    assert_eq!(result.final_program.to_string(), "t1 = x + 1\nprint t1");
}

#[test]
fn the_fixed_point_loop_terminates_within_the_cap_on_every_input() {
    let inputs = [
        "print 1\n",
        "t1 = 2 + 3\nt2 = t1 * 4\nprint t2\n",
        "a = 1\nb = a + 1\nc = b + 1\nd = c + 1\ne = d + 1\nprint e\n",
    ];
    for src in inputs {
        let prog = parse(src).unwrap();
        let result = optimize(&prog);
        assert!(result.iterations <= MAX_ITERATIONS);
        assert!(!result.hit_cap, "expected a real fixed point for {src:?}");
    }
}

#[test]
fn a_large_chain_still_terminates_within_the_cap() {
    let mut src = String::new();
    for i in 0..500 {
        src.push_str(&format!("t{i} = {i} + 1\n"));
        src.push_str(&format!("print t{i}\n"));
    }
    let prog = parse(&src).unwrap();
    let result = optimize(&prog);
    assert!(result.iterations <= MAX_ITERATIONS);
}

#[test]
fn malformed_ir_is_a_typed_error_not_a_panic() {
    let err = parse("t1 = 2 +\n").unwrap_err();
    assert_eq!(err.line, 1);
}
