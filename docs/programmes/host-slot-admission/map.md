# Host slots: a fair, measured queue

## Destination

On this machine, all three host slots fill whenever outside load allows (the
physical-core cap stays). A job never times out while the line ahead of it is
moving. Nothing heavy runs outside the queue. An agent can move urgent work
ahead, with a reason that is journalled. All of this is shown on one workday of
`waits.jsonl`, in greengate and relais alike.

## Notes

- Repositories: amont (greengate, and the `greenharness-slots` crate both tools
  link; holds this map) and relais (its coordinator and setup steps take slots
  through the same crate). Every session reads the `worktree-task`,
  `merge-when-green` and `diagnosing-bugs` skills.
- The physical-core cap is decided and stays: 6 holders × 2 jobs on 6 real cores
  drove load to 30–55 (plan "Host slots count physical cores", 2026-11-10; cap 3
  × 2 jobs on the i7-9750H). Tickets change admission, not the cap.
- Related decisions: ADR-0009 (wait, then run unqueued), ADR-0013 (a busy host
  defers), ADR-0014 (one host slot queue). Resolve them with aval before
  proposing to change them.
- Evidence, 2026-11-22 (session f06): `greengate slots` showed 1 of 3 held, load
  12.3, 5 waiting: the one holder, greengate's integration suite, kept load above
  6 by itself, so the load test (`greenharness-slots/src/lib.rs`, `poll_with`:
  `load1 < cores`) never opened slots 2 and 3. Since 2026-11-21 18:00 the journal
  (`/tmp/greenharness-slots-501/waits.jsonl`) holds 232 rows: 103 admitted, 50
  deferred, 38 timeout, 29 unqueued (8 `cargo-test` and 6 `clippy` among them,
  started at load 445–563). Load peaked at 635 between 21:00 and 23:59 that
  night, with 17 Claude sessions (10 GB) and 4.5 of 6 GB of swap in use.
- The person (2026-11-22): a queue can time out while it is only waiting in line
  as normal; agents have been seen running heavy work outside the queue;
  sometimes not waiting was what they wanted; the feature they would like is
  for the agent to reorder the queue.
- A ticket's branch is `fix/hsa-<id>` (or `docs/hsa-<id>`): amont's branch
  pattern refuses `prog/<slug>/<id>`. That branch is the in-flight signal.

## Decisions so far

- [Admission stops counting the holders' own load](t01-admission-load-rule.md): admit while `load1 < cores + 4·Σjobs(holders)`; the floor and the cap stay.
- [A wait times out only when the line stops moving](t02-no-timeout-in-a-moving-line.md): a wait ends after 300 s with no admission and no position change; the old budgets become a ceiling. 14 of the 38 recent timeouts would have been saved.
- [How heavy work gets past the queue](t04-heavy-work-outside-the-queue.md): 148 bare heavy commands advised in 7 days (greenbelay); 38 unqueued fallbacks; a hand-set held marker is unchecked.
- [How the queue could be reordered safely](t05-reorder-mechanism.md): rename the ticket to an earlier name under the admission lock; a waiter finds its ticket by `pid-seq` first.
- [A relais run keeps its place between setup steps](t07-relais-setup-keeps-its-place.md): one lease for the whole setup.
- [What a check does when its wait ends](t03-unqueued-fallback.md): a stalled hook check defers to CI and `slot run` exits 75; not waiting is an explicit, journalled up-front choice (amends ADR-0009). The person's decision.
- [Who may move a job ahead, and how far](t06-who-may-reorder.md): own session's jobs only, to the head, with a journalled reason; 3 moves per session per day, never past a job waiting 20 min or more. The person's decision.
- [The journal records holders and hold time](t09-journal-holders-and-hold-time.md): every wait line carries `holders` and `holder_jobs`, and every dropped lease leaves one `released` line with `held_ms`, written by the crate (greenharness-slots 4.13.0).
- [A hand-set held marker skips the queue](t10-held-marker-is-trusted.md): never set by hand in 7 days; the real hole is an inline marker on `amont slot run` or `git commit`. The marker becomes `held:<lease>`, checked live by one shared reader; a forged one queues with a journalled line. Build: t15.
- [The journal holds up a day after the 4.13 install](t14-journal-measure-after-install.md): met the plan's `## Measure`; p95 of `waited_ms` 1.23× baseline, no rollback.

## Handoff

2026-11-23, t11 Phases 1–3 (branch `fix/hsa-t11`, plan
`docs/plans/2026-11-23-admission-counts-holders-stalled-waits.md`). The crate,
greengate and ADR-0022 are built and tested, in one amont PR. Nothing is
released yet, and t11 stays open.

What the PR holds:

- Admission takes a further slot while `load1 < cores + 4 × Σjobs`.
- A wait ends on a 300 s stall or at its ceiling, and it defers or exits 75.
  It never runs unqueued.
- A pre-stage deferral writes an hour-long `deferred` completion, so land
  still pushes.
- `GREENGATE_ON_BUSY=now` and `slot run --now` skip the queue up front.
- `--on-timeout run` is refused.
- The journal gains `end` and the outcome `now`.
- ADR-0022 replaces the heads of `hooks.host-concurrency`,
  `hooks.busy-policy` and `hooks.gates-before-connection`.

Measured: t01's admission test fails on `origin/main` (the old rule refuses
at 12.3) and passes here. A real-window pilot exits 75 after 300 s with
`"end":"stalled"`.

Next, in order:

1. ~~t14~~ closed 2026-11-24: the 4.13 journal held (p95 1.23×, 291 rows per
   12 h, no `Drop` panic; one stale 4.11.0 writer in greenrack worktrees).
   Phase 4 may go ahead.
2. t11 Phase 4: release greengate 5.0.0 (crates.io included; a major: `cli.compat.interface-is-public`) and install it.
   Read the crates.io job's conclusion: its token is a 90-day one. Run
   semver-checks, or an equivalent check, before the publish.
3. t11 Phase 5 (relais):
   - bump the crate, `Stall` in `heavy.rs` `take()`, and `admission_overrun`,
     for the host-slot wait only;
   - the relais record that amends `execution.resource-budgets`;
   - release and install.
4. t11 Phase 6: greenbelay's `heavy-build-unqueued` learns `--now`. Then t11
   closes and [t08](t08-workday-measure.md) can start.
5. Then t12, t13, t15, and [t16](t16-coordinator-queue-moving-line.md) once
   relais ships.

The plan's "Full reviews" sidecar lists the findings carried into Phases 4–6.

Accepted gaps:

- `slot run`'s `--max-wait` and `onBusy defer`'s `slotWait` stay hard (the
  Decision log), so 6 of 50 `slot run` timeouts and 14 of 103 deferrals,
  which were in a moving line, are not saved.
- An I/O error before the queue is joined still runs unqueued ("slots off").

Lessons:

- The host was jammed all session, at load 30 to 70 with 1 to 2 holders.
  Run tests through `greengate slot run --max-wait 3000` in the background.
- A `sh -c '… | tail'` hides the failure text: write the whole log to a file
  and grep it.
- amont's pre-commit runs `onBusy defer`, so clippy and the tests defer at
  commit. Run them by hand first.

No steps are left to the person.

## Not yet specified

- Whether outside load the queue does not own (many agent sessions, swap
  pressure) needs its own defence beyond the load test, such as memory pressure
  as an admission input.
- How a reorder shows in relais's pane (greengate's view is in t13).
- A detached wait for `greengate slot run`. The Bash tool kills a command at
  120 s, so `--max-wait` stays a hard deadline (t11's Decision log), and a
  moving line longer than that still ends a wait. 6 of 50 `slot run` timeouts
  since 2026-11-21 18:00 were in a moving line.
- Whether greenbelay should flag an inline `GREENGATE_ON_BUSY=now` prefix, as
  it flags `slot run --now` (t11 Phase 6).

## Out of scope

- Raising the cap above the physical core count: decided on 2026-11-10 for a
  measured reason (see Notes), and not reopened here.
