---
status: active
branch: fix/hsa-t11
repos: [amont, relais, greenbelay]
adrs: [ADR-0022]
plan-id: p-01M83QVZ0P9JBMHBJP5R88YRME
---
# Admission counts holders; stalled waits defer

## Review panel
👉 **Decide:** none — approve if a stalled wait deferring to CI, with an hour-long completion so land still pushes, is the t03 decision built.
📍 amont, relais, greenbelay · 6 phases · 3 releases · next: Phase 1. Panel: architect, backend, rust, po, tui, unix (opus), codex.
**Changed by review:** a pre-stage deferral writes an hour-long `Completion::Deferred`; relais's overrun stays off the coordinator queue and below `exhausted()`; `now` is env-only, never committed.
📄 Full reviews: [2026-11-23-admission-counts-holders-stalled-waits.reviews.md](2026-11-23-admission-counts-holders-stalled-waits.reviews.md)
**Verdicts:** final: 3 approve, 3 approve-with-changes (low); codex rework, answered by its blocker's edit. 17 carried; words 1760 → 2327 (×1.32, blocker exempt); remove 0, merge 0.

## Picture

```
 today
 job --queue--> budget spent? --yes--> run unqueued (load 400+)
                   |
                   `--no--> load1 < cores? --> slot

 after this plan
 job --queue--> stalled 300 s, or ceiling? --yes--> defer / exit 75
                   |
                   `--no--> load1 < cores + 4 x holder jobs? --> slot
                   |
 job --now--------------------------> run unqueued (journalled)
```

A queued job used to run anyway once its budget ran out; now it waits while
the line moves and defers when it stalls.

## Goal

Ticket t11 of `host-slot-admission` builds t01 (holders' own load stops
blocking the next slot), t02 (a wait ends when the line stalls, not when a
budget runs out in a moving line) and t03 (a stalled check defers or exits
75; skipping the queue is an up-front, journalled choice). ADR-0022 records
it.

## Non-goals

- The physical-core cap, the floor (nothing held, admit) and the 60 s ramp stay
  as they are.
- The relais coordinator's model-work queue (`runner/launch.rs`, which also
  charges `admission_wait_seconds`) is a different line, with no journal. It
  gets a follow-up ticket.
- No detached wait for `slot run` (fog), and `slotWait` stays hard (Decision
  log).
- No reorder (t13), held-marker check (t15) or relais setup lease (t12).
- No memory-pressure input to admission (fog).

## Code review

- amont: backend, lang:rust, unix, architect (tui left out: stderr wording only, no screen)
- relais: backend, lang:rust, architect (unix, tui left out: no shell or screen change)
- greenbelay: backend, lang:rust (unix, tui, architect left out: one advise rule's argument parsing)

## Behaviour

### Admission (crate `greenharness-slots`, `Waiter::poll_with`)

Automatic sizing, when a slot is already held: the head is admitted when a slot
is free, the ramp has passed, and `load1 < cores + LOAD_PER_JOB × Σjobs`, with
`LOAD_PER_JOB = 4`. t01 derived it: one holder at `jobs 2` measured 12.3
against an idle baseline of about 4, so about 4 per job. ADR-0022 repeats the
arithmetic.

- `Σjobs` sums `jobs` once per lease, by its `lease` id, over the live holder
  records of the held slots.
- A held slot with no readable record, or a record without `jobs`, counts
  `parallelism(1, cores, cap)`, 2 here. One crate function gives Σjobs to
  `poll_with` and `occupancy()` (lib.rs:1072), so the journal's `holder_jobs`
  is the threshold's.
- The waiting reason uses the same threshold. `WaitReason::Load` gains
  `holder_jobs`, and `describe()` reads `load 31.0 >= 14 (6 cores + 4×2 holder
  jobs)`.
- Fixed sizing and the floor are unchanged.

### The stall clock (crate, new `Stall`)

`Stall::new(window, now_ms)` and `Stall::observe(&mut self, waiter, &queue,
now_ms) -> bool` define one rule, shared by greengate and relais:

- The idle time is `now_ms.saturating_sub(max(the start of the wait, the last
  admitted-at stamp, this waiter's last position change))`. All three marks are
  wall-clock milliseconds from the injected `now_ms`. A stamp ahead of `now`, or
  a backwards step, gives an idle time of 0, never a panic. A test covers a
  future-dated stamp.
- `observe` returns true once the idle time reaches `window`.
- A poll that missed the admission lock (`queue.reason == None`) moves no mark.
- `admitted-at` is read by the crate (`admitted_at(dir)`), so both tools read it
  the same way.
- `STALL = 300 s` is the default window. `GREENHARNESS_SLOT_STALL_S`
  overrides it, a test seam beside `GREENHARNESS_SLOT_DIR`.

### How each caller ends a wait

| Caller | Stall window | Hard ceiling | When the wait ends | Budget that no longer ends a moving wait |
|---|---|---|---|---|
| hook check, `onBusy wait` (default; also rehearsal and pre-stage) | 300 s | `greengate.timeout` (3600 s when 0) | **defers to CI** (was: runs unqueued) | `greengate.timeout` is the ceiling, as before |
| hook `warn` check in land's pre-stage | 300 s | `greengate.warnSlotWait` (600 s), capped by the ceiling | **defers** (was: runs unqueued) | — |
| hook check, `onBusy defer` | — | `slotWait` (60 s) | defers, unchanged | none (deliberate) |
| hook `warn` check outside a rehearsal | — | 0 | defers at once, unchanged | — |
| `slot run` | min(300 s, `--max-wait`) | `--max-wait` (90 s) | exits 75, unchanged | none (deliberate: Bash limit) |
| relais verification command | 300 s | the run's total budget | `deferred` row, the run blocks `admission_wait_exhausted` | `admission_wait_seconds` |

- The sibling-holder extension (an `onBusy defer` check that waits to the ceiling
  while its own process holds a slot) is kept. It now also ends on a stall.
- A check deferred in a rehearsal or a pre-stage writes a
  `Completion::Deferred` record (`refs/notes/amont-gate-done`), which answers
  for an hour, as `Unavailable` does (`gate_stamp.rs:965`). It is never a
  stamp or an attestation. Without it, land's push would refuse with
  `evidence-missing` (ADR-0018: `dispatch.rs:1580`), since a deferral records
  nothing today (`dispatch.rs:1878`). With it, land pushes and CI runs the
  gate. A changed tree or gate token voids it. An older binary does not read
  the word, so the record answers nothing there.
- The deferral text names its cause. A stall says `deferred to CI: the host slot
  line made no progress for 300 s (position N)`, and `slotWait` keeps
  `(greengate.onBusy defer)` (`host_slots.rs:338`). The roll-up
  (`dispatch.rs:2115`) gives the same cause. `run now:` advises
  `GREENGATE_ON_BUSY=now`, and the `onBusy wait` hint is dropped for stalls,
  because under ADR-0022 `wait` defers too.

### Skipping the queue up front (t03)

- `greengate slot run --now -- <cmd>` runs at once, unqueued and bounded to
  its width. The journal gets one row with outcome `now`. `--now` is a boolean
  flag. `--now=x`, or `--now` with `--max-wait`, exits 2.
- `GREENGATE_ON_BUSY=now` does the same for every heavy hook check of that run,
  with outcome `now`. A one-line note names the setting. `now` is refused in
  `greengate.onBusy` and `set onBusy` (the manifest stays `wait|defer`), so no
  committed file skips the queue. Precedence: `--now`, then `GREENGATE_ON_BUSY`,
  then `greengate.onBusy`, then `wait`; the variable does not affect `slot run`.
- `--on-timeout` is removed. `--on-timeout run` exits 2 with
  two lines, each under 80 columns: `greengate: --on-timeout was removed
  (ADR-0022): a wait that ends runs nothing` and `  skip the queue up front:
  greengate slot run --now -- <cmd>`.
  `--on-timeout exit` is accepted for one release, since it was the default,
  with a one-line deprecation note on stderr. Its exit status is unchanged.
- Exit 75 says why: `no progress in the host slot line for 300s` (stall) or
  `no host slot within --max-wait 90s` (ceiling), then `` `<label>` did not
  run; retry later, or pass --now to skip the queue``.

### relais (`verify/heavy.rs`, `policy/budgets.rs`)

- `take()` keeps a `Stall` (300 s). When `admission_wait_seconds` is spent but
  the line moved within the window, the wait goes on.
- Time past the spent admission budget is charged to a new
  `admission_overrun`, never to `admission_waited`. This keeps
  `Budgets::exhausted()` (`engine.rs:718`, `engine.rs:838`, `attempt.rs`)
  from ending the run after the grant: `exhausted_waits` tests `>=` (budgets.rs:386), so `admission_waited` is held below the limit while the line moves, and the Phase 5 test asserts `exhausted()` is None after a grant with a non-zero overrun. It is still out of work and inside the
  total.
- A stall ends the wait as the spent budget does today: a `deferred` row, then
  `blocked (admission_wait_exhausted)`, with the event reason `the host slot
  line made no progress for 300 s at position N`.
- The overrun applies to the host-slot wait alone. The coordinator queue
  (`heavy.rs:779`, `runner/launch.rs:133`) also charges
  `WaitKind::Admission`, and it keeps its exhaustion unchanged.
  `Queue::charging` settles the guard when the wait switches between the
  two. A test covers coordinator → host slot (overrun) → coordinator, where
  the coordinator still exhausts.
- The disk wait is unchanged.
- `admission_overrun` lives in the in-memory `Spent` (budgets.rs:124), like
  `admission_waited`. Nothing persists it, so no ledger migration follows.

### The journal

A wait row that did not admit carries `"end": "stalled"` or `"end": "ceiling"`,
through a new `WaitContext::end` field. relais builds `WaitContext` with
`..default()`. amont's own literal (`host_slots.rs:600`) gains the field.

### greenbelay (`heavy-build-unqueued`)

`slot run --now` counts as unqueued, as `--on-timeout=run` did. A
`--on-timeout=run` clause is still recognised: under greengate 4.14 it runs
nothing, so it is no longer advised as unqueued, and the advice names
`--now`.

## Phases

- [ ] Phase 1 — crate: admission threshold, `WaitReason::Load.holder_jobs`,
  `Stall`, unit tests (t01's two cases, and t02's red test: position 10, an
  admission every 60 s, not stalled after 400 s; its pair, stalled at 300 s):
  `crates/greenharness-slots/src/lib.rs`
- [ ] Phase 2 — greengate: `wait_for_slot` takes a window and a ceiling, with
  a clock seam; end → defer for `wait` and the pre-stage; `now` in
  `resolve_busy` and its complaint (`host_slots.rs:163,166`), but not in the
  manifest; `slot run --now` (a boolean flag kind in `parse`), the stall
  window, the two exit-75 messages; `--on-timeout` removed from the parser,
  both usage strings and the module doc (`slot_cmd.rs:4,10,33`); tests:
  `crates/greengate-runtime/src/host_slots.rs`,
  `crates/greengate-runtime/src/slot_cmd.rs`,
  `crates/greengate-runtime/src/dispatch.rs`,
  `crates/greengate-runtime/src/gate_stamp.rs`, `crates/greengate/src/main.rs`,
  `crates/greengate/tests/it/slot_run.rs`
- [ ] Phase 3 — the decision and docs: ADR-0022 replaces the heads of
  `hooks.host-concurrency` (ADR-0021) and `hooks.busy-policy` (ADR-0013), naming
  ADR-0009's lineage, and the head of `hooks.gates-before-connection`
  (ADR-0018) to add the hour-long deferral completion, taking effect with the
  greengate 4.14.0 install; `aval heads --write`:
  `adr/0022-waits-end-on-a-stalled-line.md`, `adr/HEADS.md`,
  `docs/host-slots.md`, `CHANGELOG.md`
- [ ] Phase 4 — release greengate 4.14.0 (crates.io included) and install it
  on this machine, only after t14's 24-hour window has closed and t14 is
  recorded: `Cargo.toml`, `CHANGELOG.md`
- [ ] Phase 5 — relais: bump `greenharness-slots = "4.14"`, `Stall` in
  `take()`, `admission_overrun`, tests (a moving line outlives the admission
  budget, and the run is not ended after the grant; a stalled line blocks at
  300 s), a relais record replacing `execution.resource-budgets` (ADR-0010 row `admission_wait_seconds`: a moving line outlives it, overrun charged to the total only) with `aval heads --check`, pointer plan, release and install:
  `crates/relais/Cargo.toml`, `crates/relais/src/verify/heavy.rs`,
  `crates/relais/src/policy/budgets.rs`, `CHANGELOG.md`,
  `docs/plans/2026-11-23-admission-counts-holders-stalled-waits.md`
- [ ] Phase 6 — greenbelay: `--now` counts as unqueued, and the advice
  names it; release and install:
  `crates/greenbelay-guard/src/rules/heavy_build_unqueued.rs`,
  `crates/greenbelay-guard/tests/corpus/heavy-build-unqueued.cases`,
  `docs/rules.md`, `CHANGELOG.md`

Phases 1–3 are one amont PR, merged now; no amont release is cut from main until t14 closes. Phase 4 waits for t14 (due
2026-11-24). Phases 5 and 6 need the crate on crates.io and greengate 4.14
installed.

## Decision log

- 2026-11-23 — `--max-wait` stays a hard deadline, because `slot run` lives
  inside Claude Code's 120 s Bash limit. A moving line past it would be killed
  queued, with no exit 75 to read. Re-run on the journal since 2026-11-21
  18:00: 50 `slot run` timeouts, 6 of them while the line moved. None of them
  is saved, where t02 counted 14 under its looser rule. A detached wait is the
  fix, filed as fog.
- 2026-11-23 — `onBusy defer`'s `slotWait` stays hard: 103 deferrals, 14 of them
  while the line moved. A deferral is the repository's chosen outcome, and it
  adds no load to the host, so it is not a timeout in t02's sense.
- 2026-11-23 — t03's "ADR-0009 is amended" is recorded as ADR-0022. The live
  heads of the two keys are ADR-0021 and ADR-0013 (ADR-0009's
  `hooks.host-concurrency` was replaced by ADR-0013), so the new record replaces
  those, and its text names the lineage.
- 2026-11-23 — 4.14.0 is a minor release with BREAKING entries, as 4.11.0 was.
  `WaitReason::Load` gains a field (a fleet grep finds no reader outside
  greengate), and `--on-timeout run` is refused. relais's `"4.13"` caret takes
  4.14. Not 5.0.0: the workspace version is greengate's own, so a major
  ripples every npm pin. `#[non_exhaustive]` would itself be a break.
- 2026-11-23 — A fleet grep finds one `--on-timeout` caller outside amont:
  greenbelay's `heavy-build-unqueued` rule, which Phase 6 migrates.
- 2026-11-23 — The install waits for t14: a 4.14 writer inside t14's 24-hour
  window would change the admissions and outcomes that t14 measures.

## Verification

- Phase 1: `cargo test -p greenharness-slots`. The two t01 cases (12.3 → Granted,
  300 → `Waiting(Load)`) and the two t02 cases (moving, not stalled at 400 s;
  idle, stalled at 300 s) fail on `origin/main` and pass here.
- Phase 2, piloted with the built binary on a scratch `GREENHARNESS_SLOT_DIR`
  with `greengate.hostSlots 1`:
  - A holds `slot run -- sleep 420`. B runs `slot run --max-wait 600 -- true`.
    B exits 75 at about 300 s. Its stderr names the stall, and its row reads
    `timeout` with `"end": "stalled"`.
  - Three waiters are queued ahead of B, and a holder is released every 60 s, so B's position changes. B, with `--max-wait 600`, is admitted after more than 300 s. A stall-window env override lets `slot_run.rs` cover stall → 75 in seconds.
  - `slot run --now -- true` runs at once, and its row reads `now`.
  - `slot run --on-timeout run -- true` exits 2 with the message above.
  - Hook path: clock-seam tests on `wait_for_slot`, one per table row (stall,
    ceiling, sibling-holder extension), and a test on `before_spawn`'s end
    action. Pilot: with a stall window of 5 s, a hook check behind a stalled
    holder defers and leaves no stamp. `greengate land` to a local bare
    remote, with that gate owed, pushes on the `deferred` completion, and the
    gate is absent from the attestation. A changed tree owes it again.
- Phase 3: `aval heads --check` exits 0, and `aval resolve hooks.busy-policy
  --scope hook-binary` names ADR-0022.
  - With `GREENGATE_ON_BUSY=now`, the same check runs at once, and its row
    reads `now`.
- Phase 4: `greengate --version` reads 4.14.0, and crates.io lists it.
- Phase 5: a relais command in a moving line past `admission_wait_seconds` is
  granted, and the run is not blocked.
- Phase 6: `slot run --now` triggers `heavy-build-unqueued`, and the advice
  names `--now`.
- After the 4.14 install, count the journal at 24 h and 7 d against the
  baseline since 2026-11-21 18:00: hook unqueued 34, hook defer 103, `slot run`
  timeouts 50, relais deferred 21. The targets are 0 unqueued rows after a wait
  (outside `now`) and 0 runs blocked `admission_wait_exhausted` after a grant.
  Roll back if the hook deferral rate passes 2× the baseline. This count is
  t08's workday measure, and t08 is told so.

<!-- panel: repos=amont,relais,greenbelay adds=contract reviewers=architect,backend,lang:rust,po,tui,unix,codex body-sha=7220cf1a32a9 -->
