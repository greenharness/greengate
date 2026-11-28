+++
type = "task"
mode = "afk"
status = "closed"
blocked_by = []
repos = ["amont"]
+++
# `slot run` has no user-facing way to wait past a stall

## Question

On greengate 5.x `greengate slot run --max-wait 3000 -- <cmd>` still exits 75
after the 300 s stall window when the line does not move (seen 2026-11-26
while t18 was built: clippy queued behind three holders at load 340). The only
way to wait longer is the test seam `GREENHARNESS_SLOT_STALL_S`. The driver's
lesson "run tests through `slot run --max-wait 3000`" therefore no longer
holds, and t17 counted 43 `slot run` stall timeouts. Should `slot run` take a
`--stall <s>` (or let `--max-wait` above the window mean "wait for me")?

## Look at

- `crates/greengate-runtime/src/slot_cmd.rs` (the stall window read near line
  215, the exit 75 message near 265), `docs/host-slots.md` "When a wait ends".
- t19's `slot run` timeouts by `end`.

## Answer looks like

A decision (flag, or keep the stall as is and fix the lesson), with the map's
Lessons line corrected either way.

## Resolution

2026-11-28. **No stall window for `slot run`; no `--stall` flag.** Plan
`docs/plans/2026-11-28-slot-run-waits-to-max-wait.md`, reviewed by the panel
(backend, lang:rust, tui, unix), built on branch `fix/hsa-t21` (the driver's
`prog/…` name is refused by amont's branch pattern). ADR-0022 amended.

- The window is applied only when it is shorter than the ceiling, and the
  default `--max-wait` is 90 s: the stall only ever ended a `slot run` whose
  caller asked for more than 300 s, so the ADR's "suits a command someone
  watches" did not hold.
- Journal, 2026-11-27 11:22 to 2026-11-28 13:37 CET (greengate 5.4.0): `slot
  run` 77 admitted, 9 `now`, 9 `ceiling`, 37 `stalled` (300 to 998 s, median
  391 s). 36 of the 37 were followed by a same-session `slot run` within 30
  min, which joined at the back. All 37 sat behind a holder that held 15 min or
  more (28 past 30 min), and each of those holders was released: a full line,
  not a wedged one.
- `slot run` now builds `WaitBounds { window: None }` like the hook path,
  ignores `GREENHARNESS_SLOT_STALL_S`, and exits 75 only at `--max-wait`
  (`--help`, `docs/host-slots.md` and the CHANGELOG say so). relais keeps the
  crate's stall rule.
- It ships in the greengate release after 6.0.0. Until that is installed the
  workaround stays: prefix a long `slot run` with
  `GREENHARNESS_SLOT_STALL_S=3000`.
- Check 24 h after that install: `slot run` rows with `"end":"stalled"` are 0,
  and same-session retries within 30 min of an exit 75 fall from 36.
