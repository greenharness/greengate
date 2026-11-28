# Full reviews: the queued heartbeat names the ceiling

Round 1: all four approve-with-changes. Round 2 on the final body (sha 8ef6f317): rust approve, tui approve, unix approve, backend approve-with-changes.

## Carried into implementation

1. backend (medium): in a land rehearsal (`crate::rehearsal::in_snapshot()`), `rehearsal.rs:1092` sets `GREENGATE_ON_BUSY=wait`, so an outer `=now` does not reach the check. Leave the skip clause out of the heartbeat under `in_snapshot()` and add a unit case.
2. tui (medium): write the clause as `skip the queue: rerun with GREENGATE_ON_BUSY=now`, as the roll-up says `skip the queue`. The plan's "skip:" is superseded; the test pins `skip the queue:`.
3. backend (medium, the person's call, not taken): carrying each check's own bound (60 s `slotWait`, 600 s `warnSlotWait`) into the live row needs the per-thread `CURRENT` wait plan in `host_slots`; 11 of 35 waits would read "at most 59m00s". Follow-up.
4. backend (low): the plan has no `## Outcome`; add it at the close and paste the live pilot line there.
5. rust (low): the heartbeat fires only after 60 s and only on non-tty stderr; the pilot holds a slot over 60 s and captures stderr.
6. tui (low): an admitted-after-wait check never sees the roll-up, so the heartbeat is its only hint.

## backend (opus)
Round 1: approve-with-changes (data table did not add up; live check missing; code-review reasons; per-check bound). Round 2: finding 5 (on_push wrong command in land) led to the bare variable; finding 6 above.

## lang:rust (opus)
Round 1: approve-with-changes (one resolver for the 0-means-3600 rule; exact-label tests; beat test rewrite). Round 2: found `on_push` is wrong for a land rehearsal and `run --raw` (high); approve after the fix.

## tui (opus)
Round 1: approve-with-changes (paste-ready command; "defers to CI" wording; command last). Round 2: approve; findings 2 and 4 as above.

## unix (opus)
Round 1: approve-with-changes ("defers to CI"; paste-ready skip; no ANSI in a log). Round 2: approve.
