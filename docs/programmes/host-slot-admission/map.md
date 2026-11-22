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

## Not yet specified

- Whether outside load the queue does not own (many agent sessions, swap
  pressure) needs its own defence beyond the load test, such as memory pressure
  as an admission input.
- How a reorder shows in `greengate slots` and in relais's pane, once t06
  decides what a reorder is.

## Out of scope

- Raising the cap above the physical core count: decided on 2026-11-10 for a
  measured reason (see Notes), and not reopened here.
