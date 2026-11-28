---
status: done
branch: prog/host-slot-admission/t20-queued-heartbeat-names-ceiling
repos: [amont]
adrs: [ADR-0022]
plan-id: p-01M8FH118P0RWR9B68M6X6SM0Z
---
# The queued heartbeat names the ceiling

## Review panel
👉 **Decide:** none — approve if the heartbeat naming its ceiling and the skip variable, and the roll-up label carrying the resolved length, is the whole change.
📍 amont · 1 phase · next: none, done. Panel: backend, lang:rust, tui, unix (opus); codex: not installed.
**Changed by review:** wording `defers to CI`; skip names the variable only (`on_push` cannot name the command run); one shared `wait_ceiling_secs`.
**Verdicts:** 2 approve, 2 approve-with-changes after round 2; 5 carried into implementation (listed in the reviews); words 629 → 894 (×1.42, fixes to wrong statements exempt); remove 0, merge 0.
📄 Full reviews: [2026-11-27-queued-heartbeat-names-ceiling.reviews.md](2026-11-27-queued-heartbeat-names-ceiling.reviews.md)

## Picture

```
 before: queued 1m00s for a host slot (greengate.hostSlots 2)
 after : queued 1m00s for a host slot (greengate.hostSlots 2),
         defers to CI in at most 59m00s (greengate.timeout);
         skip the queue: rerun with GREENGATE_ON_BUSY=now
```

The picture shows the one line that changes: the queued heartbeat.

## Goal

A hook check can wait up to `greengate.timeout` (3600 s) in a host-slot line,
and its heartbeat says only `queued 1m00s for a host slot (greengate.hostSlots 2)`.
The person cannot tell when the wait ends nor how to skip it. The heartbeat
should name both, and the deferral roll-up label should carry the resolved
length instead of the bare setting name (t20, carried from t18's tui review).

## Data: how long hook waits really are

Window: `/tmp/greenharness-slots-501/waits.jsonl` from 2026-11-26 16:24 CET
(the 5.1.1 install) to its last row, 2026-11-27 14:23 CET (about 22 h); hook
rows only (`tool = "greengate"`, `released` lines left out): 35 waits.
t19 (due 16:24) has not run; this is not its verdict.

| group | n | p50 | p95 | max | over 60 s | over 300 s |
| --- | --- | --- | --- | --- | --- | --- |
| all hook waits | 35 | 106 s | 896 s | 1104 s | 27 | 14 |
| admitted | 20 | 367 s | 965 s | 1104 s | 16 | 12 |
| deferred (all `end: ceiling`) | 11 | 60 s | 600 s | 600 s | 11 | 2 |
| handed (a free slot, no queue) | 4 | 0 s | 0 s | 0 s | 0 | 0 |

Admitted hook checks waited a median 6 minutes and up to 18 minutes in a
line that shows no end. So the message is needed.

## Non-goals

- The 80-column region row (`· queued 42s (slots 2/2)`): unchanged.
- The wait itself (ceilings, stall, `slotWait`, `warnSlotWait`), the journal.
- A per-check exact end: a `warn` check ends at `warnSlotWait` (600 s) and an
  `onBusy defer` check at `slotWait` (60 s), both earlier than the ceiling.
  The text says "at most", which is true for every one. Cost, carried: 11 of
  the 35 waits were bounded at 60 s, and their first heartbeat reads "at most
  59m00s" as they defer. Carrying each check's own bound into the live row
  means changing `host_slots`; left to a follow-up.

## Code review
- amont: backend, lang:rust, unix, tui (matches `greenbelay plan-panel --code-review`: a CLI repository, one Rust phase; no UI, platform or React profile applies)

## Behaviour

The heartbeat (a log line, after its unchanged prefix) becomes
`… queued 1m00s for a host slot (greengate.hostSlots 2), defers to CI in at most 59m00s (greengate.timeout); skip the queue: rerun with GREENGATE_ON_BUSY=now`.
The added text is plain, with no
`highlight()` and no escape bytes: the heartbeat is a log.

- "in at most" is `ceiling - queued secs` (never below 0), the ceiling being
  `greengate.timeout`, or 3600 when it is 0, resolved by one new
  `host_slots::wait_ceiling_secs(timeout)` that `host_slots.rs` (where it is
  inline now), `beat_line` and both `announce_deferred` call sites share.
  `Budgets.ceiling` keeps 0 = off (the 80% cliff note reads it).
- The skip names the variable only. `on_push` is false for a pre-push check
  in a land rehearsal (live.rs:985) and true under `greengate run --raw`, and
  stage names carry `pre-push-` either way, so no field names the command that
  was run; the variable is true on every path but a land rehearsal, which forces
  `onBusy wait` on its snapshot: there the clause is left out (the reviews'
  sidecar). Rerunning is the only way, as
  the choice is made up front (ADR-0022). The roll-up already prints the whole
  command for the deferred checks.
- The roll-up label reads `wait for a slot, up to 1h00m (greengate.timeout):`
  with the resolved ceiling, within 80 columns.

## Phases

- [x] Phase 1 — heartbeat and roll-up: `crates/greengate-runtime/src/live.rs`
  (`beat_line` queued branch, test `a_queued_check_says_it_waits_for_a_host_slot`
  rewritten to the new exact line, extra cases with `Budgets { ceiling: …, ..B }`),
  `crates/greengate-runtime/src/host_slots.rs` (`wait_ceiling_secs`),
  `crates/greengate-runtime/src/dispatch.rs` (`announce_deferred` and
  `deferral_advice` take the ceiling; their tests),
  `crates/greengate/tests/it/busy_defers.rs` (label assertion); one commit.
No release is part of this plan: a message change only, shipped with the next greengate release.

## Decision log
- 2026-11-27 — the roll-up label is changed too, because the ticket's Look-at
  asks for it and the ceiling is already resolved at both call sites.

## Verification
- Phase 1, `beat_line`: a queued row at 60 s with ceiling 3600 prints the exact
  line (`… defers to CI in at most 59m00s (greengate.timeout); skip the queue:
  rerun with GREENGATE_ON_BUSY=now`); 3590 s queued → `10s`, 7200 s → `0s`,
  ceiling 0 → `59m00s`; on a first beat the clause ends the line after the kill
  rule and before the push note; inside a rehearsal snapshot it is absent and
  "defers to CI" stays; no `\x1b` → observed: `a_queued_check_says_it_waits_for_a_host_slot` matched the exact line,
  `a_queued_note_counts_down_to_the_ceiling` the `10s`/`0s`/`59m00s` cases, and
  `the_way_out_of_a_queue_ends_the_line_and_is_withheld_in_a_land` the order and
  the withheld text (889 lib tests pass).
- Roll-up: `deferral_advice` tests assert the exact label
  `  wait for a slot, up to 1h00m (greengate.timeout):`, timeout 0 gives the
  same, all lines at most 80 columns; `busy_defers` (its repo sets timeout 600)
  sees `up to 10m00s (greengate.timeout):` on its own line → observed:
  `the_run_now_advice_for_a_deferred_warn_gate_waits` found the label
  `up to 10m00s (greengate.timeout):` on its own line (26 `busy_defers` tests pass).
- Live pilot: not run. The host was jammed all session (load 300+, a three-
  slot line of other sessions' suites), and the heartbeat needs a hook check
  that runs 60 s behind a held slot. The exact-line tests stand in; the first
  real queued hook will show the line on stderr.
- Toolchain 1.94.1 (`~/.cargo/bin`): `cargo fmt --check`, `cargo clippy
  --workspace --all-targets -- -D warnings` clean. The Homebrew rustc 1.100
  fails clippy on two untouched lines in `posix_signal.rs`, so the pin matters.

## Implementation review
- implementation-review: approve-with-changes, then the delta; fixed: ambient `in_snapshot()` read (now `Stage.way_out`), misplaced doc comments, a vacuous test, a flag-argument test wrapper (deleted); candidate `r` fine.
- backend, lang:rust, unix, tui: approve after the delta; low items kept: one `for_snapshot` arm swap untested, `in_snapshot()` called twice at stage build.
- Final tree (rebased on origin/main, 6ffa6ed0): implementation-review approve; rust approve (3 low notes); tui approve; unix approve-with-changes, one medium: "skip the queue" is false for a `warn` check under `warnSlotWait`. deliberate: rejected, because `wait_bounded_for_warn` runs only inside a rehearsal snapshot (dispatch.rs `enter`), where the way out is already withheld; no path reaches `BoundedForWarn` outside a snapshot. implementation-review's two lows (no test of `WayOut::for_snapshot`'s arms, no `beat_line` case at a non-3600 ceiling) are recorded deliberate: both are covered by the direct `queued_note` and `WayOut::Withheld` tests, and the wiring is checked at live.rs.
- backend (final): see the closing commit's review round.

## Outcome
Shipped on this branch (not yet released): the queued heartbeat names its
ceiling and the way out, and the roll-up label carries the resolved length.
Not done: each check's own bound in the live row (11 of 35 hook waits were
60 s ones), and a live pilot. Surprises: the first beat appends a kill rule
that would have followed the way out, so the clause moves to the line's end;
`on_push` cannot name the command run, so the clause names the variable only;
a land rehearsal forces `onBusy wait`, so there the clause is left out.

<!-- panel: repos=amont reviewers=backend,lang:rust,tui,unix body-sha=8ef6f3170ea2 -->
