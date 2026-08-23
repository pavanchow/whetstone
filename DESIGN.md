# Design

## The IR

Whetstone programs are a flat list of three-address instructions. There is no control flow, no branches, no jumps, and no function boundaries. That is deliberate: it keeps every pass a single linear scan instead of a dataflow fixed point, so the optimizer stays readable and the reasoning behind each pass stays a sentence long.

Two instruction shapes exist:

```
dst = value              // a copy, e.g. t2 = t1, or t2 = 5
dst = value op value      // a binary operation, e.g. t2 = t1 + 3
print value               // the only instruction with a side effect
```

A value is either an integer literal or a register name. A register that is never assigned, like `x` in `t2 = x * 2`, is a free variable, an input to the program. It can never be folded away because Whetstone has no idea what it holds.

Operators are `+ - * /` and the six comparisons, which produce `0` or `1`.

The parser is hand-written: split each line into tokens, dispatch on the second token being `=` or the first token being `print`, and reject anything else with a `ParseError` carrying the 1-based line number. There is no lexer generator and no grammar file, the grammar is small enough to read directly in the match arms.

## The passes

Every pass has the same shape: `Program -> (Program, Vec<String>)`, a pure function from one program to the next plus a list of human-readable notes describing what changed. No pass panics and no pass mutates in place, which keeps `--show-passes` able to show a real before and a real after for each one.

**Constant folding.** Scans for an assignment whose right-hand side is `Const op Const` and replaces it with the single computed constant. Division by zero is left unfolded on purpose, so the program can still fail at runtime the way it would have without optimization, instead of the optimizer silently deciding what happens.

**Constant propagation.** Walks the program forward carrying a map from register name to the constant it is currently known to hold. Because the IR has no branches, this map is exact, not an approximation: an instruction is reached exactly once and in exactly one order, so "this register currently holds 5" is a fact, not a guess. Every operand that names a register in the map gets replaced by that constant before the instruction is processed. When an assignment's right-hand side is not a bare constant, the destination register is removed from the map, since anything reading it afterward should see the real, unknown value again.

**Algebraic simplification.** A small table of identities on binary operations: `x + 0`, `0 + x`, `x - 0`, `x * 1`, `1 * x`, and `x / 1` all become a bare copy of `x`, and `x * 0` or `0 * x` become the constant `0`. This runs independently of folding, an identity involving a free variable like `x * 1` is never a foldable constant expression, but it is still an algebraic identity.

**Dead code elimination.** A single backward scan. Starting from the end of the program with an empty set of "still needed" registers, walk instructions in reverse. `print` is always kept, and it marks its operand as needed. An assignment is kept only if its destination is currently marked needed, or if it has a side effect (nothing currently does, since `print` is not an assignment, but the check exists so a future side-effecting instruction is not silently deleted by mistake). A kept assignment marks its own operands as needed and un-marks its own destination, since anything upstream can no longer see that name.

## The fixed-point pipeline

The four passes run in this fixed order every round: fold, propagate, simplify, eliminate. A round that produces no change anywhere ends the pipeline. A round that changes anything runs again, because these passes feed each other: propagating a constant can expose a new fold, folding can turn an assignment into dead code, and removing dead code can occasionally shorten a chain enough for propagation to reach further than it could before.

The loop is capped at a fixed number of rounds. On IR without cycles, which is all Whetstone IR, since there is no way to write a loop in this language, the pipeline provably reaches a true fixed point in at most one round per instruction in the program, so the cap is never the reason a real program stops. It exists as a hard backstop: if a bug ever made two passes fight over the same rewrite forever, the compiler still terminates and reports that it hit the cap instead of hanging.
