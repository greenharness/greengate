---
status: done
branch: fix/hsa-t21
repos: [amont]
adrs: [ADR-0022]
plan-id: p-01M8H30C8113Z0WMD49KTQ5J5F
---
# slot run waits to its --max-wait

## Review panel
👉 **Decide:** none — approve if `slot run` ending only at its own `--max-wait`, with no stall window and no new flag, is the fix.
📍 amont · 2 phases · next: Phase 1. Panel: backend, lang:rust, tui, unix (opus).
**Changed by review:** help line, Commands entry and seam sentence added to Phase 1; rollback, consequence and 24 h check added; stall test replaced, moving-line test deleted.
**Verdicts:** 3 approve + 1 approve-with-changes after round 2; 1 carried (a Verification check for the deleted test and help line); words 827 → 1054 (×1.27); remove 0, merge 0.
codex: not installed.
📄 Full reviews: [reviews](2026-11-28-slot-run-waits-to-max-wait.reviews.md)

## Picture

```
 slot run --max-wait 3000, line full for 300 s
 before: stall window 300 s -> exit 75 -> retry joins at the back
 after : no stall window -> keeps its place; admitted, or exit 75 at 3000 s
```

The picture shows the one change: a `slot run` wait ends at its own
`--max-wait`, never on a stall.

## Goal

`greengate slot run --max-wait S` waits in line for up to `S` seconds and
nowhere less: the 300 s stall window no longer ends its wait. A caller that
asks for a long wait (a background `slot run --max-wait 3000 -- cargo test`)
keeps its place in the line instead of exiting 75 and re-queueing at the back.
Programme `docs/programmes/host-slot-admission`, ticket t21.

## Non-goals

- relais keeps the crate's stall rule (its verification waits are bounded by
  a run budget and a cancel); the `greenharness-slots` crate does not change.
- The hook path is unchanged (it lost its stall window in 5.1.1, t18).
- No `--stall <s>` flag: no caller has asked for a shorter end than its own
  `--max-wait`, and a flag adds surface for a case the evidence below does not
  show.
- The default `--max-wait` (90 s, under the Bash tool's 120 s) and exit 75 at
  the ceiling stay. `--now` stays.
- No release: this ships in the greengate release after 6.0.0 (another
  session cuts 6.0.0).

## Code review
- amont: backend, lang:rust, unix (tui left out: no new message; the stall line is no longer printed, and a long wait repeats the 30 s "behind" line up to --max-wait/30 times)

## Behaviour

Why the stall window goes, from the journal
(`/tmp/greenharness-slots-501/waits.jsonl`, 2026-11-27 11:22 to 2026-11-28
13:37 CET, greengate 5.4.0 installed):

- `slot run` wait lines: 77 admitted, 9 `now`, 9 `timeout`/`ceiling`, 37
  `timeout`/`stalled`.
- A window is applied only when it is shorter than the ceiling
  (`WaitBounds`), and the default ceiling is 90 s. So a stall can only end a
  `slot run` whose caller passed `--max-wait` above 300 on purpose, and so
  ran it in the background. All 37 stalled rows waited 300 s or more (median
  391 s, max 998 s). ADR-0022's reason for keeping the stall on `slot run`,
  "the stall end suits a command someone watches", does not hold: a watched
  foreground command cannot reach 300 s.
- 36 of the 37 stalls were followed by another `slot run` from the same
  session within 30 minutes: a retry that joined at the back of the line. Most
  stalls came with 3 of 3 slots held at load 8 to 15, behind long holders such
  as a 28-minute `cargo test`: the line was full, not broken. Several waiters
  stalled at the same instant (they share the admission stamp) and all
  re-queued.

After the change:

| Caller | Stall window | Ceiling | At the end |
|---|---|---|---|
| `greengate slot run` | none | `--max-wait` (90 s) | exits 75 (74 when the queue failed) |

- `slot run` builds `WaitBounds { window: None, ceiling: max_wait }`, as the
  hook path does (the `LineEnd::Stalled` arms stay, since `LineEnd` is
  exhaustive: commented as unreachable with no window, as `host_slots.rs` is,
  still exit 75, no `unreachable!()`; the `WaitBounds` comment is rewritten), and no longer reads `GREENHARNESS_SLOT_STALL_S` (the crate's
  test seam stays for relais and the crate's own tests). The CHANGELOG entry
  names the variable as ignored by `slot run`, since no warning is printed.
- A wait ends at `--max-wait` with exit 75, `no host slot within --max-wait
  Ns`, and a journal line `"outcome":"timeout","end":"ceiling"`; or on three
  failed polls with exit 74, as before.
- The module doc and `docs/host-slots.md` say so: "When a wait ends" (the table row, the sentence that `slot run` keeps the stall rule, and the seam sentence, now relais only) and the Commands entry for `slot run` (no "line stalls for 300 s"); ADR-0022
  gets an amendment and its `hooks.host-concurrency` choice is updated
  (`adr/HEADS.md` regenerated with `greentopo heads --write`).
- Consequence, in the amendment: a long `--max-wait` behind a wedged holder now waits to its ceiling (every one of the 37 stalls sat behind a holder that held 15 min or more, 28 behind one past 30 min, and each of those holders was released: none was wedged). Rollback: reinstall the previous greengate and note it in ADR-0022. Trigger: any `slot run` wait over 30 min whose holder's command never exited.
- After install, 24 h: `slot run` rows with `"end":"stalled"` = 0, and same-session `slot run` retries within 30 min of a 75 fall from 36 toward 0, recorded in Outcome.

## Phases

- [x] Phase 1 — `slot run` has no stall window:
  `crates/greengate-runtime/src/slot_cmd.rs`,
  `crates/greengate/tests/it/slot_run.rs`,
  `crates/greengate/src/main.rs` (help: exit 75 only when --max-wait runs out),
  `adr/0022-waits-end-on-a-stalled-line.md`, `adr/HEADS.md`,
  `docs/host-slots.md`, `CHANGELOG.md`
- [x] Phase 2 — the ticket closes: `docs/programmes/host-slot-admission/t21-slot-run-waits-past-a-stall.md`,
  `docs/programmes/host-slot-admission/map.md` (Decisions line, and the
  Lessons line on long `slot run` waits)

## Decision log

- 2026-11-28 — no stall window for `slot run` rather than a `--stall` flag,
  because the stall window only ever fires against an explicit `--max-wait`
  above 300 s, and 36 of 37 stalls were retried at once (ticket t21 lists
  both options; the ticket is afk, so the agent decides).
- 2026-11-28 — branch `fix/hsa-t21`, not `prog/host-slot-admission/t21-…`:
  the map's Notes say amont's branch pattern refuses `prog/`.

## Verification

- Phase 1, regression test (replaces `a_stalled_line_exits_75_before_the_max_wait`;
  holder `sleep 20`, killed after; also asserts no `no progress in the host slot
  line`): a holder keeps the only slot, the waiter runs
  `slot run --max-wait 5` with `GREENHARNESS_SLOT_STALL_S=2` → exit 75 after
  at least 5 s, `no host slot within --max-wait 5s`, `"end":"ceiling"` in the
  journal, nothing run → observed: `cargo test -p greengate --test it slot_run` (through `slot run --max-wait 3000`): 17 passed, including `a_full_line_ends_a_wait_at_the_max_wait_never_on_a_stall` (exit 75 at 5 s or later, ceiling row, no stall line).
- Phase 1, piloted run: the branch binary, a scratch `GREENHARNESS_SLOT_DIR`,
  one holder `slot run -- sleep 20`, then a waiter `slot run --max-wait 15
  -- true` with `GREENHARNESS_SLOT_STALL_S=3` → exits 75 at about 15 s, not 3
  s → not run as a separate pilot; the regression test above covers it (STALL_S=2 against a 5 s ceiling).
- Phase 1, the moving-line test (`a_moving_line_outlasts_the_stall_window`)
  is deleted: with no window, its `elapsed > 4 s` check proves nothing, and the
  regression test shows the env seam is ignored. Check: `grep moving_line slot_run.rs` → no match; `target/debug/greengate --help | grep -i stall` → only `install` lines → observed.
- Phase 1: `greentopo heads --check` on the amended ADR → exit 0, and `git diff adr/HEADS.md` changes only the `hooks.host-concurrency` choice → observed: `greentopo heads --check` exit 0, one line changed in HEADS.md.
- Phase 1, the whole suite: `greengate slot run --max-wait 3000 -- cargo test -p greengate --test it` → no regression elsewhere (busy_defers and land still set the seam for the hook path) → observed: 821 passed, 0 failed (1855 s).

## Outcome

Shipped on `fix/hsa-t21`: `slot run` waits to its `--max-wait` with no stall
window; ADR-0022 amended; help, docs and CHANGELOG say so. Not shipped: the
release (it rides the greengate release after 6.0.0) and the 24 h check after
that install. Surprise: the stall window could never fire on a default `slot
run`, only against a caller who asked to wait longer.

## Implementation review

- implementation-review: approve-with-changes, then approve on the Delta; fixed the map's Next list (closed t21) and the pilot row's wording. 55k + 28k, 62 s + 17 s.
- backend: approve; one low (5 s real wait in the test) accepted as is. 50k, 40 s.
- lang:rust: approve; one low fixed (comment on the unreachable `Stalled` arm of `end_message`). 46k, 21 s.
- unix: approve; one low kept, deliberate: `end` keeps `stalled` for relais rows, and the CHANGELOG already names the change. 49k, 33 s.

<!-- panel: repos=amont reviewers=backend,lang:rust,tui,unix body-sha=b760ff8249ed -->
