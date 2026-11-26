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
- [The journal 24 hours after the 5.0 install](t17-count-after-5-0-install.md): hook deferrals were 4.5× baseline (141.8 per 12 h against 31.8), past the 2× trigger. The person chose a narrow fix over a revert: 5.0, relais 0.83 and greenbelay 6.15 stay. Plan: t18.
- [Build: admission counts holders, waits end on a stalled line](t11-build-admission-and-waits.md): shipped in greengate 5.0.0, relais v0.83.0 and greenbelay 6.15.0; the hook path narrowed in 5.1.1 (t18); its measure is t19.
- [Hook waits end at the ceiling, not on a stall](t18-plan-hook-stall-narrow-fix.md): greengate 5.1.1 (5.1.0 was already out) gives a hook check no stall window, so it waits to its ceiling and defers there; `slot run`, `--now`, `holders` and relais stay on crate 5. Ceiling deferrals stay deferred (229 of 261 are `onBusy defer`'s 60 s `slotWait`; only 14 were unqueued before 5.0). ADR-0022 amended. The 24 h count is [t19](t19-count-after-5-1-1-install.md), and the plain 2× rule may still fire on `slotWait` rows.
- [Build: an agent can move a job ahead](t13-build-reorder.md): `greengate slot promote <id> --reason …` (5.2.0) moves a session's own waiting job up to the head under t06's limits, journalled `promoted`; a waiter finds its ticket by inode, and `slots` lists the queue. relais's jobs become movable only once it links crate 5.2, and under its `run_id` session.

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
2. ~~t11 Phase 4~~ done 2026-11-24: greengate 5.0.0 released (Release run
   https://github.com/greenharness/greengate/actions/runs/45386975691:
   conclusion success, with `publish to crates.io`, `publish to npm`, `publish
   to homebrew tap` and `brew can install what was published` each success)
   and installed on this machine (brew; `greengate --version` printed
   `greengate 5.0.0`). cargo-semver-checks is not
   installed; the major bump and the changelog carry the breaks. Observed:
   `cargo search` lists `greengate-runtime` and `greenharness-slots` 5.0.0;
   `npm view @greenharness/greengate` reads `latest` 5.0.0 (a few minutes
   after the green job); the brew binary is Cellar/greengate/5.0.0. Pins left
   at the old major on purpose until each repo's own bump (read from the
   `package.json` files; the plan's Decision log says `^4`): `^4.11.0` in
   duro-app and greenrack, and relais's `greenharness-slots = "4.13"`, which
   Phase 5 moves to `"5"`. The
   24 h and 7 d journal counts against the baseline run from this install
   (about 2026-11-24 12:49): they are t17's count, carried into t08 when it holds. Hook shims and the
   AGENTS.md block are not refreshed fleet-wide yet.
3. ~~t11 Phase 5~~ done 2026-11-25: relais v0.83.0 (`greenharness-slots = "5"`,
   `Stall` in the host-slot wait, in-memory `admission_overrun`, ADR-0015;
   relais #573, release #580) installed 2026-11-25 02:26 CET; coordinator
   stopped then. t17's `admission_wait_exhausted`-after-grant count starts there.
4. ~~t11 Phase 6~~ done 2026-11-25: greenbelay 6.15.0 (`heavy-build-unqueued`
   fires on `slot run --now` and advises it; #331, release #333) installed
   2026-11-25 about 06:00 CET. t11's code is all shipped, but its plan stays
   active: the post-install count is t17's. Its interim count has already
   fired the plan's 2× trigger: 158 hook deferrals in the 13.4 h after the
   2026-11-24 12:49 install (141.8 per 12 h) against a baseline of 31.8 per
   12 h (103 deferrals from 2026-11-21 18:00 over 38.9 h), 4.5× (commit
   c5acb1f0 on the unmerged branch `docs/hsa-t17-count`, owned by another
   session). t17's verdict and the rollback are pending there; do not count
   again here. t08 waits on t17. (Superseded by items 5 and 6.)

   greengate 4.13 has no `--now`, and relais 0.83 admits by crate 5's rule, so
   a rollback has three parts: greengate back to 4.13 (t17's recipe),
   greenbelay back to 6.14.0 or forward without the `--now` advice, and
   relais back to 0.82.0 or a stated decision to keep it on crate 5. Which
   parts, and in what order, is t17's decision.
5. ~~t17~~ closed 2026-11-25: the 2× trigger fired, and the person chose a narrow fix, with no revert of greengate, relais or greenbelay. Next was [t18](t18-plan-hook-stall-narrow-fix.md), with no stalled-line deferral on the hook path. t08 counts from the 5.1.1 install (t19).
6. ~~t18~~ closed 2026-11-26: greengate 5.1.1 (hook waits have no stall end; #526, release #527, Release run 45847436802 success) installed 2026-11-26 16:24 CET; t19 is due from 2026-11-27 16:24. t11 closed with it: all its code is shipped and its measure is t19. Next: [t19](t19-count-after-5-1-1-install.md), the first half of t08; [t20](t20-queued-heartbeat-names-ceiling.md) (low).
7. ~~t13~~ closed 2026-11-26: greengate 5.2.0 (`slot promote`; #530, release #531, Release run 45889748609 success) installed 2026-11-26 21:21 CET. No relais change. Follow-ups filed: [t22](t22-count-after-5-2-0-install.md) (7-day count, due 2026-12-03), [t23](t23-promote-off-switch.md) (off switch, plus the plan's low review findings), [t24](t24-relais-jobs-can-be-moved.md) (relais jobs movable, after t12), and `follow-ups/f12-slot-flag-allowlist.md` (outside the Destination).
8. In flight 2026-11-26 (driver session f3dfb52d): t12 (relais, its release is v0.90.0 after another session's v0.89.0). [t16](t16-coordinator-queue-moving-line.md), unblocked by t11, counts 7 days after the relais v0.83.0 install (2026-11-25 02:26), so it is dated 2026-12-02. t15 starts after t12, as both change the held marker relais exports.

The plan's "Full reviews" sidecar lists the findings carried into Phases 4–6.

Accepted gaps:

- `slot run`'s `--max-wait` and `onBusy defer`'s `slotWait` stay hard (the
  Decision log), so 6 of 50 `slot run` timeouts and 14 of 103 deferrals,
  which were in a moving line, are not saved.
- An I/O error before the queue is joined still runs unqueued ("slots off").

Lessons:

- The host was jammed all session, at load 30 to 70 with 1 to 2 holders.
  Run tests through `greengate slot run --max-wait 3000` in the background. From 5.0 a
  `slot run` also exits 75 after a 300 s stall: start it with
  `GREENHARNESS_SLOT_STALL_S=3000` ([t21](t21-slot-run-waits-past-a-stall.md)).
- A `sh -c '… | tail'` hides the failure text: write the whole log to a file
  and grep it.
- amont's pre-commit runs `onBusy defer`, so clippy and the tests defer at
  commit. Run them by hand first.

No steps are left to the person.

## Not yet specified

- Whether outside load the queue does not own (many agent sessions, swap
  pressure) needs its own defence beyond the load test, such as memory pressure
  as an admission input.
- A detached wait for `greengate slot run`. The Bash tool kills a command at
  120 s, so `--max-wait` stays a hard deadline (t11's Decision log), and a
  moving line longer than that still ends a wait. 6 of 50 `slot run` timeouts
  since 2026-11-21 18:00 were in a moving line.
- Whether greenbelay should flag an inline `GREENGATE_ON_BUSY=now` prefix, as
  it flags `slot run --now` (t11 Phase 6).

## Out of scope

- Raising the cap above the physical core count: decided on 2026-11-10 for a
  measured reason (see Notes), and not reopened here.
