---
status: active
branch: fix/hsa-t18
repos: [amont]
adrs: [ADR-0022]
plan-id: p-01M8BJT90PMW7CX01RGA602AGC
---
# Hook waits end at the ceiling, not on a stall

## Review panel
👉 **Decide:** none — approve if a hook wait ending at its ceiling, never on a stall, with ceilings still deferring, is the narrow fix built.
📍 amont · 3 phases · 1 release · next: Phase 1. Panel: backend, lang:rust, tui, unix (opus).
**Changed by review:** roll-up label and command on their own lines (80 columns); ceiling cause names `greengate.timeout` or `warnSlotWait`; tests lowered to a 3 s timeout, `stall_window()` deleted.
**Verdicts:** 3 approve-with-changes, 1 approve after round 2 (tui's last finding applied); 4 carried (heartbeat t20, `joined` row, hour-long waits, interrupted waits unmeasurable); words 1072 → 1428 (×1.33, fixes to wrong or unverifiable statements exempt); remove 0, merge 0 (one rejected).
codex: not installed.

## Picture

```
 hook check, onBusy wait          slot run / relais
 before: stall 300 s -> defer     stall 300 s -> exit 75 / block
 after : no stall, ceiling -> defer   (unchanged)
```

The picture shows the one change: a hook check no longer stops at the stall window.

## Goal

greengate 5.0 made a hook check defer to CI when its line made no progress for
300 s. After the install, in t17's 13.4 h window, that was 75 of 158 hook deferrals (4.5× the pre-5.0
rate, past the plan's 2× rollback rule; programme host-slot-admission t17).
The stall end suits a command a person waits at (`slot run`, relais), not a
commit or a push that can simply wait as it did before 5.0. This release takes
the stall end off the hook path only. A hook check then waits in line until
its ceiling, as before 5.0, and defers at the ceiling, never running unqueued.

## Non-goals

- `greengate slot run`'s stall end and exit 75, `--now`, `GREENGATE_ON_BUSY=now`,
  `greengate slots` holders, the journal's `end` field, and the crate's
  `wait_in_line` stall rule all stay. relais stays on crate 5 and its own stall.
- No revert of 5.0, relais 0.83 or greenbelay 6.15.
- `onBusy defer`'s `slotWait` (60 s) and `warnSlotWait` (600 s) stay as the
  hard ceilings they are.
- The 5.0 admission rule (`cores + 4 × Σjobs`) is untouched.

## Code review
- amont: backend, lang:rust, unix, tui

## Behaviour

Rows of ADR-0022's wait table, hook path only, after this change:

| Caller | Stall window | Ceiling | At the end |
|---|---|---|---|
| hook check, `onBusy wait` (and every rehearsal) | none | `greengate.timeout` (3600 s when 0) | defers to CI |
| heavy `warn` check of land's pre-stage | none | `greengate.warnSlotWait` (600 s, never above the timeout) | defers |
| hook check, `onBusy defer`, and its sibling extension | none | `slotWait` (60 s), then to the ceiling while a sibling holds | defers |
| `slot run`, relais | 300 s | unchanged | unchanged |

The code change is in `crates/greengate-runtime/src/host_slots.rs`:
`wait_plan` passes `window: None` for the `onBusy wait` and `BoundedForWarn`
rows, and `sibling_bounds` passes `window: None`; the two `stall_window()` call
sites go with them, so a bad `GREENHARNESS_SLOT_STALL_S` warns only from
`slot run`. The `stall` parameter is
removed from `wait_plan` and `sibling_bounds`, and `stall_window()` is deleted
(dead code under `-D warnings`; `slot run` reads the seam itself,
`slot_cmd.rs`). The `sibling_bounds` unit test and the doc comments that say
"while the line moves" follow.

What the person sees changes with it: the roll-up's `run now:` line
(`crates/greengate-runtime/src/dispatch.rs`) is relabelled `wait for a slot, up to greengate.timeout:` on its own line, the command alone on the next (deeper indented, so it copies whole and fits 80 columns; the `skip the queue` line takes the same shape), since that command no longer ends after 300 s, and the
`onBusy wait` ceiling cause names `greengate.timeout`
(`deferred to CI: no host slot within 1h00s (greengate.timeout)`). A land
pre-stage `warn` check also runs as `BusyPolicy::Wait`, so `defer_cause` takes
the `SlotWait` too and names `greengate.warnSlotWait` for `BoundedForWarn`;
`a_deferral_names_its_cause` asserts both ceiling rows. The queued
heartbeat in `live.rs` (naming the ceiling and `GREENGATE_ON_BUSY=now`) is
not built here; it is filed in Phase 3 as ticket
`t20-queued-heartbeat-names-ceiling`.

### The 261 ceiling deferrals: they stay deferred

Decision: a ceiling still defers. It does not return to an unqueued run.

Evidence (journal, hook rows of `tool=greengate` since the 5.0 install, to
2026-11-26 09:14 UTC, 428 deferrals: 167 `stalled`, 261 `ceiling`). The
261 ceiling rows are not 3600 s waits: 229 end at 60 s (the repository's own
`onBusy defer` `slotWait`), 14 at 600 s (`warnSlotWait`), 18 at 0 s. None
ended at `greengate.timeout`. A 60 s `slotWait` deferral was a deferral before
5.0 too (ADR-0013), so those rows are no regression and not a former unqueued
run. Only the `onBusy wait` ceiling used to run unqueued, and with no stall
end it is now the single way a default-policy wait ends, 3600 s after the line
joined.

Against greenbelay's `heavy-build-unqueued` (advise, 6.15) and the programme
Destination, "nothing heavy runs outside the queue", unqueued runs at the
ceiling are the defect the programme exists to remove: 29 unqueued runs
started at load 445 to 563 in the 2026-11-21 window. Bringing them back
would also contradict `slot run --now` being the only, said, journalled way
past the queue. Recommended and taken: keep deferring, and stop the stall end.
Rejected: (b) unqueued at the `onBusy wait` ceiling only (restores the
pre-5.0 overload path for 3600 s waiters, which are rare once stall no
longer fires); (c) a longer stall window (a number to tune, and the line
still defers while it is moving slowly).

Risk accepted: a commit can wait up to `greengate.timeout` in a stalled line,
as before 5.0; the deferral roll-up still offers `GREENGATE_ON_BUSY=now`.

## Phases
- [x] Phase 1 — hook waits have no stall window; the roll-up (label and command on their own lines, and its `deferral_advice` and `DeferredBy` comments), and the ceiling cause say the new end; tests follow: `crates/greengate-runtime/src/host_slots.rs`, `crates/greengate-runtime/src/dispatch.rs`, `crates/greengate/tests/it/busy_defers.rs`, `crates/greengate/tests/it/land.rs`
- [x] Phase 2 — amend ADR-0022 for the hook path and docs: `adr/0022-waits-end-on-a-stalled-line.md`, `adr/HEADS.md`, `docs/host-slots.md` (a hook wait ends at its ceiling; the stall applies to `slot run` and relais only)
- [ ] Phase 3 — changelog and tickets done; the release follows the merge (`tag-release`, 5.1.1): `CHANGELOG.md`, `docs/programmes/host-slot-admission/t18-plan-hook-stall-narrow-fix.md`, `docs/programmes/host-slot-admission/t19-count-after-5-1-1-install.md`, `docs/programmes/host-slot-admission/t20-queued-heartbeat-names-ceiling.md`, `docs/programmes/host-slot-admission/t21-slot-run-waits-past-a-stall.md`, `docs/programmes/host-slot-admission/t08-workday-measure.md`, `docs/configuration.md`, `docs/programmes/host-slot-admission/map.md`

## Decision log
- 2026-11-26 — release number is 5.1.1, not 5.0.1: main already carries 5.1.0
  (#525, installed here), and a lower tag would sort below the published
  latest on crates.io, npm and brew. The fix is a patch on 5.1.0.
- 2026-11-26 — ceiling deferrals stay deferred (see Behaviour), because the
  Destination forbids heavy work outside the queue and most are the
  repository's own 60 s `slotWait`.
- 2026-11-26 — ADR-0022 amended in place for the hook-path clause rather than
  a new record: the decision's other clauses are unchanged.

## Verification
- Unit: `wait_plan` rows for `onBusy wait` and `BoundedForWarn` carry
  `window: None`; `sibling_bounds` carries `None`; `a_deferral_names_its_cause`
  asserts `greengate.timeout` and `greengate.warnSlotWait`.
- Integration: the busy_defers ceiling test asserts the roll-up says
  `wait for a slot, up to greengate.timeout:` and no longer `run now:`, and that its roll-up lines are at most 80 columns.
- Integration: a hook check beside a never-released holder, with
  `GREENHARNESS_SLOT_STALL_S=1` and a fixture `set timeout 3` (never 0, which
  means 3600 s), is still waiting past the old stall window (elapsed at least
  3 s) and defers at the ceiling, journalled `"end":"ceiling"` and never
  `stalled`;
  a land pre-stage gate (fixture `set timeout 4`, stall seam 2 s) defers at its ceiling,
  journalled `"end":"ceiling"`, and still pushes with a `deferred`
  completion; `slot run` still exits 75 with `"end":"stalled"`
  (`slot_run.rs` unchanged).
- Count, 24 h after the 5.1.1 install (the first half of t08), over
  `waits.jsonl` wait rows only (`jq -cR 'fromjson? | select(has("waited_ms"))'`):
  hook rows with `"end":"stalled"` are 0 (the pass condition of this fix),
  and a hook wait killed by Ctrl-C writes no end row, so interrupted waits
  are reported as not measurable: `greengate slots --json` lists holders but
  no waiter pids, and dead records are deleted by every admission pass. A
  `joined` journal row is a follow-up if the count matters;
  p95 and max `waited_ms` of hook wait rows, and the count ending at
  `greengate.timeout`, are reported. A hook wait reaching 3600 s is a finding
  for the person (t08), not an automatic rollback;
  unqueued rows after a wait outside `now` are 0; and hook deferrals per 12 h
  are reported beside the 31.8-per-12 h baseline, split by cause (`slotWait`
  60 s, `warnSlotWait` 600 s, `greengate.timeout`, and the former stalled).
  Expectation, stated now so it is not a surprise: the ceiling rows alone ran
  at about 69 per 12 h since the 5.0 install (261 in 45.4 h, 2.2× baseline),
  229 of them the old `onBusy defer` `slotWait` path under a host whose hook
  wait rows doubled (112 to 227 per 12 h) and whose median load rose from 11
  to 17. So the plain 2× rule (63.6 per 12 h) can still fire after this fix
  without 5.1.1 being the cause. If it does, and the excess is `slotWait`
  rows, the verdict is for the person (t08): it needs a like-for-like
  baseline or a decision on `slotWait`, not another stall change. The count
  is a dated task ticket, `t19-count-after-5-1-1-install`, due the install
  time plus 24 h, and the first half of t08.

## Decision log (build)
- 2026-11-26 — t21 filed during the build: `slot run` exits 75 after the 300 s stall even with `--max-wait 3000`, so the driver's test recipe needs `GREENHARNESS_SLOT_STALL_S=3000`. The roll-up label prints once above the commands. The 80-column claim assumes a check name under about 35 characters. Whether the label should also show the resolved timeout is asked in t20.

## Outcome
Verification observed before the push (host jammed, run through `greengate slot run` with the stall seam at 3000 s):
- clippy `-D warnings` on greengate-runtime and greengate: clean.
- `cargo test -p greengate-runtime --lib -- host_slots:: dispatch::`: 30 passed. The `wait_plan` rows carry `window: None`, `sibling_bounds` carries `None`, `a_deferral_names_its_cause` asserts `greengate.timeout` and `greengate.warnSlotWait`.
- `cargo test -p greengate --test it -- busy_defers:: land:: slot_run::`: 54 passed, including `a_hook_check_has_no_stall_end_under_onbusy_wait` (stall seam 1 s, `set timeout 3`: waited at least 3 s, `"end":"ceiling"`), `a_gate_deferred_at_the_ceiling_still_lets_land_push` (fixture `set timeout 4`, stall seam 2 s), and `slot_run::a_stalled_line_exits_75_before_the_max_wait` unchanged. After the review fixes the busy_defers and land tests were run again: 41 passed, then the one failing assertion (a cause text a rehearsal does not print) was removed and the test re-run, passed.
- The 24 h count is t19.

## Implementation review
- implementation-review: approve-with-changes, round 1; delta approve-with-changes (plan-record items only, fixed here); 2 stale doc comments fixed; 5 findings.
- backend: approve-with-changes, then approve; a `warn` pre-stage integration assertion added; label printed once.
- lang:rust: approve; 3 low comment and naming fixes. unix: approve, no findings.
- tui: approve-with-changes, then approve; label once; deliberate: the label names `greengate.timeout` without its value, and `skip the queue` names the first check only (pre-existing shape).

<!-- panel: repos=amont reviewers=backend,lang:rust,tui,unix body-sha=6802c27a7313 -->
