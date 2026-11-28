+++
type = "task"
mode = "afk"
status = "open"
blocked_by = []
repos = ["amont"]
+++
# Homebrew rust 1.100 shadows the rustup 1.94.1 pin, so clippy fails on posix_signal.rs

## Question

Homebrew rust 1.100 sits first on PATH and shadows the rustup 1.94.1 pin, so
clippy fails on `posix_signal.rs` with `suspicious_runtime_symbol_definitions`
on `read` and `write`. Should the signatures be fixed to `c_void`, or should
the gate's cargo honour `rust-toolchain.toml`?

## Look at

- `crates/greengate/src/posix_signal.rs`: the `read` and `write` definitions
  the lint names.
- `rust-toolchain.toml`, and how the pre-commit gate finds cargo and clippy.
- `which -a cargo` on this machine (Homebrew before `~/.cargo/bin`). The
  workaround until this is decided is `PATH=$HOME/.cargo/bin:$PATH` in front
  of the commit.

## Answer looks like

A recorded choice between the two fixes in the Resolution, and the chosen fix
merged, so the gate passes with the stock PATH and clippy runs on the pinned
1.94.1.
