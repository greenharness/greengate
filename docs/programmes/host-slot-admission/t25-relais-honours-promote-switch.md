+++
type = "task"
mode = "afk"
status = "open"
blocked_by = ["t24-relais-jobs-can-be-moved"]
repos = ["relais"]
due = "2026-12-03"
+++
# relais honours the promote off switch

## Question

greengate 6.0.0's off switch (`greengate.hostPromote 0`, or
`GREENGATE_HOST_PROMOTE=0`; [t23](t23-promote-off-switch.md)) lives in
`greenharness-slots` 6. relais pins `greenharness-slots = "5.5"`
(`crates/relais/Cargo.toml:67`), so a move made through relais (t24) ignores
the switch. That makes t13's rollback only partial. Should relais bump to
crate 6 and check the switch before any move, and does that need to land
before [t22](t22-count-after-5-2-0-install.md)'s count on 2026-12-03, which
relies on the switch as the rollback?

## Look at

- relais `crates/relais/Cargo.toml` and `Cargo.lock` (the pin), and what t24
  merged for relais's promote path.
- greengate `crates/greenharness-slots/src/host.rs` (`resolve_promote`,
  `greengate.hostPromote`) and `src/reorder.rs` (`Refused::Off`): what a caller
  must call before it promotes.
- greengate 6.0.0's CHANGELOG, for the breaking changes a 5.5 → 6 bump picks up
  (#543 touches `greengate-fleet`, not the crate).

## Answer looks like

- A relais change that bumps to `greenharness-slots` 6 and refuses a move when
  the switch is off. It needs a test with `GREENGATE_HOST_PROMOTE=0`, and a
  live check: the installed relais refuses with a `promote_refused` row
  carrying `"limit":"off"`.
- Or a recorded reason why relais's moves can stay outside the switch, with
  t13's rollback line amended to say so.
- Either way, a line for t22 saying whether its count waits for this.
