+++
type = "task"
mode = "afk"
status = "open"
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
