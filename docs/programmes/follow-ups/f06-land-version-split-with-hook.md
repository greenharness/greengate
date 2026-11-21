+++
type = "task"
mode = "afk"
status = "open"
blocked_by = []
repos = ["amont"]
+++
# Land refuses at once when the hook runs another greengate

## Question

`greengate land` stamps a built-in gate under a token that carries greengate's own version
(`gate_identity::builtin_token`). The push that land makes runs the repository's pre-push hook,
which runs the binary baked into the hook. When the two binaries differ in version, the
pre-stage says "already stamped on this tree — nothing to rehearse". The hook then looks for a
token under its own version, finds none and refuses with `greengate-land: evidence-missing`.
`greengate rehearse --wait` says the tree is stamped, so following the refusal's advice loops.

Hit 2026-11-21 in greenrack. A `pnpm install` in one of its worktrees ran
`prepare: greengate init`, which re-baked the shared `.git/hooks` to that worktree's
`node_modules` greengate (the repository's devDependency, 4.11.0). Every other worktree of the
repository then pushed through 4.11.0, while land on `PATH` was 4.12.0. Re-baking with
`/usr/local/bin/greengate install` fixed it.

Two things to decide: should land compare its own version with the baked hook's, and refuse up
front, naming the split and the re-bake command (or run the hook's binary for the pre-stage)?
And should `greengate init` from a worktree's `node_modules` re-bake the shared hooks of every
worktree?

## Look at

- `crates/greengate-runtime/src/gate_identity.rs` (`builtin_token`)
- `crates/greengate-runtime/src/dispatch.rs` (`land_refusal`, the `decide` closure)
- `crates/greengate-runtime/src/rehearsal.rs` (`vouched_by`, "already stamped on this tree")
- the hook shim's `BAKED=` resolution order, and what `greengate init` writes when run from `node_modules`

## Answer looks like

A land whose hook would run a different greengate version refuses before the pre-stage, with
one line naming both versions and the re-bake command (or the pre-stage runs the hook's binary,
so both agree). A test reproduces the split: two binaries, stamp from one, push through the other.
The decision on `init` from a worktree's `node_modules` is recorded.
