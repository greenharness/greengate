+++
type = "task"
mode = "afk"
status = "closed"
blocked_by = []
repos = ["amont"]
+++
# An off switch for `greengate slot promote`

## Question

The t13 plan's `## Measure` names a switch that turns `slot promote` off, but
5.2.0 does not build one, so the only rollback today is reinstalling greengate
5.1.1. Which setting turns promotion off (a `greengate.*` key, an environment
variable, or both), and what does a refused promote print and journal while
it is off?

## Look at

- `docs/plans/2026-11-26-agent-moves-own-job-ahead.md` (`## Measure`, the
  rollback line) and ADR-0022's 2026-11-26 section.
- `crates/greenharness-slots/src/reorder.rs` (`promote_in`, `Refused`) and the
  greengate `slot promote` command.
- The plan's deliberate low review findings, which can ride the same change:
  no hint after a `no-room` refusal, the session variable list copied by hand
  into a hint, no test for the `--` operand, the hints, the 1 s lock-wait
  notice or the 10 s lock-busy exit 75, the I/O error's cause printed twice, a bad promote still creating
  the slot directory.

## Answer looks like

A merged greengate change and release with the switch, a `Refused` variant
(`#[non_exhaustive]` allows it) for "promotion is off", a test for it, and the
plan's rollback line pointing at the switch.

## Resolution

Closed on `fix/hsa-t23` (plan `docs/plans/2026-11-27-promote-off-switch.md`).

- The setting is a host key, `greengate.hostPromote` (`0` off, `1` or unset on,
  `--global` or `--system`), with the environment `GREENGATE_HOST_PROMOTE=0`
  turning it off for one process. The environment can only turn promotion off:
  a `1` never undoes a host `0`. Shape chosen by the ticket agent; reasons are in
  the plan's Decision log.
- A refused promote prints `refused (off): promotion is off on this machine` with
  a `set by:` line naming the source, exits 1, and journals one `promote_refused`
  row with `"limit":"off"` and `"source"` (`env`, `global` or `system`). It creates
  no slot directory and writes no row into a directory that is not private.
- The plan's low findings are folded in (no-room hints, session list from the
  crate, I/O cause once, `--` operand test, lock-wait and lock-busy tests,
  no directory created by a refusal before the queue).
- Verified: `cargo test -p greenharness-slots` (116 passed), `greengate-runtime`
  (892 passed, one fixed assertion), `greengate` slot integration tests (22
  passed, including the two new ones). `cargo clippy -D warnings` passes except
  for `suspicious-runtime-symbol-definitions` in `posix_signal.rs`, which is
  pre-existing on `origin/main` and not touched here (rustc 1.100 lint).
- Phase 2 done on 2026-11-28. #541 shipped in greengate 6.0.0 (tag v6.0.0 on
  1eb9b1c3, release PR #551, cut by the topo-spotter-rename t28 session) and is
  installed here (`greengate --version` prints 6.0.0). `greenharness-slots`
  6.0.0 is on crates.io; relais still pins `"5.5"`. Live check on the
  installed binary: `GREENGATE_HOST_PROMOTE=0 greengate slot promote 999999
  --reason …` printed `refused (off): promotion is off for this process`, then
  `set by: GREENGATE_HOST_PROMOTE=0 in the environment`, and exited 1. It
  journalled one `promote_refused` row with `"limit":"off","source":"env"`.
  The same call without the variable was refused `not-queued`. The t13 plan's
  Outcome now names the switch as the rollback
  (`docs/plans/2026-11-26-agent-moves-own-job-ahead.md`).
- When the environment set it, the refusal reads "for this process". The key
  was checked too, with a scratch `GIT_CONFIG_GLOBAL` holding `hostPromote = 0`:
  it printed `refused (off): promotion is off on this machine`, then `set by: git
  config --global greengate.hostPromote 0`, and wrote a row with
  `"source":"global"`.
- Gap: relais pins crate 5.5, so the switch does not stop moves made through
  relais until relais bumps to `greenharness-slots` 6:
  [t25](t25-relais-honours-promote-switch.md), due with t22's count on
  2026-12-03.
