+++
type = "task"
mode = "afk"
status = "closed"
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

## Resolution

Land compares before its pre-stage, inside `prestage` (so `land --tag` gets it
too), after the early returns for a push that owes no gate or has stamp reuse
off. `land_hook::split` reads the repository's pre-push shim through
`install::installed_here` (which follows `core.hooksPath` and the shared hooks
of a linked worktree), resolves the binary the shim would exec in the shim's
own order (`$GIT_HOOKS_BIN`, the baked path, `~/.local/bin`, `PATH`), and when
that binary is not land's own file, asks it `--version` under the 5-second
probe bound. A different version string refuses with one line naming both
versions and the hook's binary, then three next steps: `<land's binary> init`
(re-bake the hooks), `greengate land` again, or `<the hook's binary> land`.

The versions are compared as strings: `builtin_token` seals
`CARGO_PKG_VERSION` verbatim, so any difference is a split. Every question it
cannot answer lets land go on as before: a hook that is not our shim (none, or
hand-written), a binary that does not answer, an answer that is not
`greengate X.Y` or `amont X.Y`. Running the pre-stage with the hook's binary
was not taken: an older binary may not know land's pre-stage contract
(`GREENGATE_LAND_PRESTAGE`), and a refusal names the fix where a silent switch
would hide it.

The test, `land::land_refuses_before_the_prestage_when_the_hook_runs_another_version`,
bakes a second binary into the shim that answers `greengate 0.0.1` and logs
every other call: land exits 1 with both versions named, the gate never ran,
the other binary was asked its version only, and nothing was pushed.

**`greengate init` from a worktree's `node_modules`: unchanged.** git shares
one hooks directory across a repository's worktrees, so a re-bake from any of
them is a re-bake for all, and the binary `init` bakes is the repository's own
pinned devDependency, which is the version the team means (ADR-0019: a
repository pins its tools). Making `init` stand down in a linked worktree would
leave a fresh clone's first worktree install unable to repair the hooks. The
harm was never the bake itself but the silent split at the push, which land now
names. A shared hook pointing into a removed worktree's `node_modules` falls
through the shim's chain to `~/.local/bin` and `PATH`, which land's resolver
follows the same way.
