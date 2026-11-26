+++
type = "task"
mode = "afk"
status = "open"
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
  into a hint, no test for the `--` operand, the hints and the 1 s lock-wait
  notice, the I/O error's cause printed twice, a bad promote still creating
  the slot directory.

## Answer looks like

A merged greengate change and release with the switch, a `Refused` variant
(`#[non_exhaustive]` allows it) for "promotion is off", a test for it, and the
plan's rollback line pointing at the switch.
