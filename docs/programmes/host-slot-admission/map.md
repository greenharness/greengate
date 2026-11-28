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
- [Build: one host slot lease for a relais setup](t12-build-relais-setup-lease.md): relais v0.90.0 keeps one host slot across a setup's steps (relabelled per step, held marker exported); the test shows one `slots::take` for 3 steps and one admission-budget spend. The slot is given back before any coordinator wait, so a contended coordinator still re-queues per step.
- [The queued heartbeat names the ceiling and the way out](t20-queued-heartbeat-names-ceiling.md): `queued 1m00s … defers to CI in at most 59m00s (greengate.timeout); skip the queue: rerun with GREENGATE_ON_BUSY=now`, and the roll-up label carries the resolved length (`up to 1h00m`). Hook waits in the first 22 h of 5.1.1: p95 896 s, max 1104 s, so the line was needed. Not done: each check's own bound (60 s `slotWait`, 600 s `warnSlotWait`).
- [The journal 24 hours after the 5.1.1 install](t19-count-after-5-1-1-install.md): held. Over 26.3 h under 5.2.0–5.4.0 (5.1.1 alone ran 5 h and rotated out), 0 stalled hook rows, 0 rows without `holders`; hook deferrals 37.5 per 12 h beside 31.8. Hook wait rows halved (113.8 per 12 h beside 227); whether that is Ctrl-C aborts is t08's question. `slot run` ended 37 of 132 waits on a stall (t21).
- [`slot run` waits to its `--max-wait`](t21-slot-run-waits-past-a-stall.md): no stall window and no `--stall` flag (ADR-0022 amended 2026-11-28). The stall fired only on an explicit `--max-wait` above 300 s; 36 of 37 stalls re-queued at the back, all behind holders of 15 min or more. Ships after 6.0.0.

- [An off switch for `slot promote`](t23-promote-off-switch.md): `greengate.hostPromote 0` (or `GREENGATE_HOST_PROMOTE=0`, off-only) refuses every move with `refused (off)`, a `set by:` line and one `promote_refused` row carrying its `source`; the t13 rollback line points here once 5.6.0 ships.

- [Build: the held marker names a live lease](t15-build-held-marker-lease.md) (in progress): readers accept `held:<lease>` and bare `held` (greengate 5.4.0, relais v0.94.0); writers emit `held:<lease>` (relais v0.96.0 installed 2026-11-28, greengate 5.6.0 with t23). Phase 3, bare `held` refused, runs 24 h after the later of those two installs and 20 accepted markers; not before 2026-11-29.

## Handoff

2026-11-28, boundary: the t15 phases 1–2 and t23 PRs are merged, and t20 is closed.

Merged this wave:

- t15 phases 1–2: greengate #536, #537 (release 5.4.0, installed), #542, #544, #545; relais #642, #647 (v0.94.0), #649, #651 (v0.96.0, installed 2026-11-28 10:54 CET).
- t23 (off switch): #541.
- t20 (heartbeat ceiling): #547, closed.
- The gate race fix: #546, test-only, for f15.

Live state:

- greengate on this machine is still 5.4.0. #541, #542, #544, #546 and #547 are unreleased; they ship in greengate 6.0.0, which the topo-spotter-rename t28 session (perso-1e) is cutting with its breaking #543. The drive's own 5.6.0 release branch (worktree amont-wt-rel-560) is to be dropped once v6.0.0 is tagged.
- t15's phase-3 gate opens 24 h after the later of the relais 0.96.0 install and the greengate 6.0.0 install, with 20 accepted `held:<lease>` markers. The floor is 2026-11-29 10:54 CET.

Held: t23 is held for the 6.0.0 release (not ours). After it: install, live check, point the t13 plan's rollback line (`docs/plans/2026-11-26-agent-moves-own-job-ahead.md`) at the switch, and correct t23's ticket. Its ticket says `status = "closed"`, but its Resolution says the release is still to do. If the person or the next session finds it closed too early, set `status` back to `"open"`.

Next, in order:

1. t19 (overdue since 2026-11-27; its count window should now account for the 6.0.0 install).
2. t24.
3. t23's close-out after 6.0.0.
4. t15 phase 3, when due.

Accepted gaps:

- t15's missing round-1 plan-review ledger rows.
- #546 fixes the race by fault injection, but does not explain why only the real gate failed 4 of 4.

Lessons:

- Run long tests as `greengate slot run --max-wait 3000 -- <cmd>` with `run_in_background`. On greengate 6.0.0 and earlier, also prefix `GREENHARNESS_SLOT_STALL_S=3000`, or the wait ends after 300 s of a full line (t21). From the release after 6.0.0, `--max-wait` alone is enough.
- Prefix cargo and commit commands with `PATH=$HOME/.cargo/bin:$PATH`: Homebrew rust 1.100 shadows the pinned 1.94.1 and breaks clippy in the gate.
- A subagent cannot `gh pr merge` (the publish-without-skill hook), so the driver merges.
- Another session's unqueued test loop pushed the host to load 370; ask that session to queue it.
- A full disk (ENOSPC) stops every agent; sweep worktrees between waves.

Steps left to the person: none.

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

- Why the real pre-commit gate alone reproduced the `slot_run` race 4 of 4 (f15), where the test alone did not.
- Release coordination when several sessions release greengate at once. A version claim might avoid the 5.6.0/6.0.0 collisions seen this wave.

## Out of scope

- Raising the cap above the physical core count: decided on 2026-11-10 for a
  measured reason (see Notes), and not reopened here.
