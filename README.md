<img src="docs/logo.svg" alt="Whetstone logo" width="96">

# Whetstone: a compiler optimizer in Rust

Whetstone is a compiler optimizer written from scratch in Rust with a readable pass pipeline: constant folding, constant propagation, algebraic simplification, and dead code elimination, run to a fixed point over a tiny three-address intermediate representation. Unlike an optimizer buried inside a bigger compiler, it is standalone, so you can run it on your own input and watch each pass rewrite the code round by round. It is a clear reference for how classic compiler optimizations work and interact.

**[Live demo](https://pavanchow.github.io/whetstone/)** · MIT licensed · written in Rust

Built from scratch by [Pavan Nallamothu](https://pavanchow.github.io/) ([LinkedIn](https://www.linkedin.com/in/pavanchow/), [GitHub](https://github.com/pavanchow)).

Most optimizers are a black box bolted inside a bigger compiler. Whetstone is not attached to anything. It is a small intermediate representation and a pass pipeline you can read start to finish, run on your own input, and watch rewrite itself one pass at a time.

## What it is

Whetstone parses a tiny three-address IR, text in and text out, and runs four optimization passes over it to a fixed point:

- **Constant folding**, `t1 = 2 + 3` becomes `t1 = 5`.
- **Constant propagation**, a register known to hold a constant is substituted at every use.
- **Algebraic simplification**, `x * 1` and `x + 0` become `x`, `x * 0` becomes `0`.
- **Dead code elimination**, an assignment whose result is never read again, and that has no side effect, is dropped.

The four passes run in sequence, over and over, until a full round changes nothing, with a hard iteration cap so a pathological input can never make the pipeline spin. Folding can expose a dead assignment, and removing dead code can expose a fresh chance to propagate a constant, so it usually takes a few rounds before nothing moves.

## The IR

```
t1 = 2 + 3
t2 = t1 * x
t3 = t2 + 0
print t3
```

One instruction per line. A line is either an assignment (`dst = value` or `dst = value op value`) or a `print`. Values are integer literals or register names. Operators are `+ - * /` and the comparisons `== != < <= > >=`. Blank lines and `#` comments are ignored. Bad input is a typed parse error with a line number, never a panic.

## Usage

```sh
whetstone opt prog.ir
```

Prints the optimized IR, followed by a list of every fold, propagation, simplification, and removal that happened, on stderr.

```sh
whetstone opt prog.ir --show-passes
```

Prints the before and after of every pass that changed something, round by round, ending with the fixed point.

## Try it

There is a live, in-browser port of the same parser and passes at `docs/index.html`.

## Build and test

```sh
cargo build
cargo test
```

## License

MIT licensed. By Pavan Nallamothu.
