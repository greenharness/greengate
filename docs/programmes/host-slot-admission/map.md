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

## Handoff

2026-11-23, t10 closed (research, docs only). It found no hand-set uses in 7
days. The ticket's premise was half right: greenbelay still advises an inline
marker on `cargo test`, but stays silent on an inline marker before
`amont slot run`, `amont run` or `git commit`, and those skip the queue.
greengate, relais and greenbelay each read the marker differently today. The
decided format, check, rollout and fixture list are in t10's Resolution. The
build is filed as [t15](t15-build-held-marker-lease.md).

Earlier, t09 shipped greengate/greenharness-slots 4.13.0 and relais v0.78.0,
both installed. Plan: `docs/plans/2026-11-23-slot-journal-records-holders.md`.

Due 2026-11-24, [t14](t14-journal-measure-after-install.md): 24 hours of the
journal after the 4.13 install:
- 4.13 writers carry the fields, and the first relais line is in;
- the unpaired-admitted count;
- growth under 3× the baseline;
- the p95 waited guard.

Next, in order: t14 when due, then the builds t11, t12, t13 and t15. t15 can
share t11's crate release, but its three rollout steps each need an install
before the next. Then t08, which reads the journal as its "Look at" notes say.
No steps are left to the person.

Lessons:
- Under a load of 240 to 350 the pre-commit clippy and test checks deferred
  to CI. Run `cargo clippy -D warnings` by hand before the push.
- `cargo-semver-checks` is not installed on this machine.
- A slot test that builds `Holder::here` records the worktree path, so match
  JSON keys with their quotes, never bare words.
- Probe a greenbelay rule with the installed binary
  (`greenbelay hook` on a PreToolUse payload, with a scratch
  `CLAUDE_CONFIG_DIR`). It settles what the rule does in seconds.

## Not yet specified

- Whether outside load the queue does not own (many agent sessions, swap
  pressure) needs its own defence beyond the load test, such as memory pressure
  as an admission input.
- How a reorder shows in relais's pane (greengate's view is in t13).

## Out of scope

- Raising the cap above the physical core count: decided on 2026-11-10 for a
  measured reason (see Notes), and not reopened here.
