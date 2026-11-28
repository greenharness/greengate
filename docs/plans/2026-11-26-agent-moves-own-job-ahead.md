---
status: done
branch: fix/hsa-t13
repos: [amont]
adrs: [ADR-0022]
plan-id: p-01M8C7ZZ8GMKGY6HFJD9MZG2G6
---
# An agent moves its own job ahead

## Review panel
👉 **Decide:** none — approve if a session moving only its own waiting job, journalled and bounded by t06's limits, is the reorder built.
📍 amont · 3 phases · next: Phase 1. Panel: architect, backend, lang:rust, po, tui, unix (opus), codex.
**Changed by review:** a waiter finds its ticket by inode, not `pid-seq`; the `promoted` row is written before the rename; dead tickets listed `gone` via a read-only flock.
**Verdicts:** round 2: 1 approve, 5 approve-with-changes; codex approve-with-changes (1 high applied); 17 carried; words 1316 → 1939 (×1.47, fixes to wrong statements exempt); remove 0, merge 0.
📄 Full reviews: [2026-11-26-agent-moves-own-job-ahead.reviews.md](2026-11-26-agent-moves-own-job-ahead.reviews.md)

## Picture

```
 queue/, sorted by name          greengate slot promote 444-0 --reason …
 0 …0100-111-0 other, 4m          | lock `admission` (lock_within 10 s)
 1 …0200-222-0 other, 3m          | own session? 3 moves in 24 h?
 2 …0300-333-0 other, 2m          | passes no job waiting >= 20 min?
 3 …0400-444-0 MINE   ---rename-->  …0099-444-0 (flock follows the inode)
                                  + "promoted" row in waits.jsonl
 after: 0 …0099-444-0  1 …0100-111-0  2 …0200-222-0  3 …0300-333-0
```

The picture shows one move: the ticket is renamed to sort first, under the
admission lock after its row is journalled, and each waiter finds its ticket
again by its inode.

## Goal

The person asked for an agent to be able to reorder the host-slot queue
(programme host-slot-admission, t05 mechanism, t06 rules decided by the person).
After this change an agent session can move one of its own waiting jobs ahead,
up to the head, with a reason journalled in `waits.jsonl`. It may do so 3 times
per 24 h, and never past a job that has waited 20 min or more. `greengate slots`
shows the queue (who waits, for how long, under which id), so the agent can
name the job to move.

## Non-goals

- relais's coordinator queue stays as it is (t06). relais is not changed here:
  its waiters, on crate 5.1 until it bumps its lock file, write empty tickets,
  which can never be moved and are read correctly when passed.
- No moving of another session's job, the person's included (t06). A caller
  with no session id cannot move anything.
- No change to admission, the cap, the load test, the stall rule or the
  journal's existing lines.
- No way to move a job backwards, and no cap on moves per job beyond the daily
  budget.
- The relais pane (map: "Not yet specified") is not built here.

## Code review
- amont: backend, lang:rust, unix, tui, architect

## Behaviour

**Ticket.** A waiter's ticket keeps its name, `queue/{nanos:024}-{pid}-{seq}`,
and now holds one JSON object written into the hidden `.new-` file before the
rename that makes it visible: the waiter's `Holder` fields plus `joined_ms`,
in a crate-private record (`Holder`, which relais builds with a struct
literal, gains no field). An empty ticket is an older client's (crate ≤ 5.1).
No dependency is added: the record reuses `Holder::to_json`, `from_json` and
`escape`, and `Cargo.toml` is untouched (the version is the workspace's).

**A waiter finds itself by inode.** `Waiter` keeps the (dev, inode) of the
ticket file it holds locked, which a rename keeps and no other waiter shares,
even one in another pid namespace with the same `{pid}-{seq}`. Under the
admission lock a poll lists the live tickets and finds its own by that key. A
missing own ticket is an `io::Error` (`NotFound`, "ticket <id> is gone from
the queue"), never position 0. Leaving removes the ticket found by inode,
looking once more if a move renamed it in between. `Waiter::id()` returns
`{pid}-{seq}`, the name the CLI uses for a job.

**Promotion** (`promote_in(dir, &Promote { id, session, reason, to, tool,
now_ms })` returns `io::Result<Result<Moved, Refused>>`; `Refused` is
`#[non_exhaustive]` and `limit()` gives the token below). Under the admission
lock, taken with `lock_within(admission, 10 s)`:

1. refused `no-reason` when the reason is empty after trimming;
2. refused `no-session` when the caller's session is empty;
3. refused `not-queued` when no live ticket has that id, `ambiguous` when two
   do;
4. refused `old-client` when the ticket is empty or unreadable;
5. refused `own-session` when the ticket's `session` is not the caller's;
6. refused `already-ahead` when its position is at or before `to` (default 0);
7. refused `budget` when the caller's session already has 3 `promoted` rows in
   `waits.jsonl` with `at_ms` in the last 24 h (rolling, no time zone). The
   scan reads the journal under the lock: about 460 lines a day (232 per 12 h
   baseline), about 14k unrotated after a month; Phase 1 measures its time.
   The scan splits records at each `{"at_ms":` start, not only at newlines,
   so a fragment a failed best-effort write left cannot swallow the row
   after it; an unreadable journal is an error (exit 74), never zero moves;
8. refused `age-floor` when any job between `to` and the ticket's position has
   waited 20 min or more (`joined_ms` from its content, else the name's
   nanos); the refusal names that job, its wait, and the furthest `--to` that
   passes no such job, when one moves the ticket at all;
9. the new name sorts just before the job at `to`: the head's nanos minus 1 s
   at the head, else the midpoint between that job's nanos and the one before
   (refused `no-room` when they differ by less than 2, as two tickets that
   arrived in the same nanosecond do). The id part is kept, so the new name
   collides with no other ticket and `rename(2)` replaces nothing;
10. the `promoted` row is appended first, in one write through a new fallible
    append (wait lines stay best-effort): `at_ms`, `tool`, `pid`, `session`,
    `"outcome":"promoted"`, `id`, `from`, `to`, `passed`, `command`,
    `reason`. A failed append renames nothing and is an error. Then the
    rename; if it fails, the row stands (a counted move that did not happen,
    the budget's safe side) and the error says so. No signal between the two
    can leave an unjournalled move. The row has no `waited_ms`, so the
    journal's wait-line filter (`select(has("waited_ms"))`) skips it.

A refusal appends one best-effort `"outcome":"promote_refused"` row (`id`,
`session`, `limit`), never counted toward the budget, so the Measure can count
which limits agents hit. Each refusal names its limit and, where one exists,
the next command, e.g.
`refused (age-floor): 444-0 would pass 111-0, which has waited 23m (limit 20m)`
then `  try: greengate slot promote 444-0 --to 2 --reason <text>`. `budget`
names when the next move frees (`in 3h12m`); `no-session` names the variables
`session_id()` reads.

**What other readers see.** A waiter passed by a move reads a larger `Behind`;
its stall clock counts that as a change of place, as any position change is
(ADR-0022's stall rule is unchanged). `Status` and the `greengate slots`
count are unchanged; the queue comes only from `queue_in`. A crate ≤ 5.1
waiter (relais 0.88, old greengate binaries) keeps its cached path, which
stays right because its empty ticket is never moved.

**CLI.**

- `greengate slot promote <id> --reason <text> [--to <n>]`: the session is
  read at the binary's edge (`session_id()`). Exit 0 prints to stderr
  `moved 444-0 from 3 to 1, past 2 jobs; 2 of 3 moves left in 24 h` (every
  message goes to stderr: the exit status is the result, and the queue view
  is the one stdout a script reads).
  Refusals and errors go to stderr: a refusal exits 1, slots off or
  unsupported exits 1 saying so, a usage error exits 2, an admission lock
  still busy after 10 s exits 75 with the command to retry (a wait over 1 s
  first prints `waiting for the admission lock…` once), an I/O failure (queue
  or journal) exits 74. `docs/host-slots.md` lists the codes once; the `slot`
  usage line lists `promote`.
- `slot run`'s first wait message also prints its id and the move:
  `  id 444-0; move ahead: greengate slot promote 444-0 --reason <text>`.
- `greengate slots` lists the queue under `waiting: N`, one line per job:
  `  0  444-0  3m  0b1c2d3e  greengate slot run: cargo test (amont-wt-x)`:
  position, id, wait and session first, then the command cut with `…` so each
  line stays within 80 columns (a test: 7-digit pid, 200-character command).
  `old client` for an empty ticket, `gone` for a dead one.
- `greengate slots --json` gains `queue`: `[{"id","position","waited_ms",
  "session","tool","command","repo","gone"}]`, `session` to `repo` `null` for
  an old client's ticket; the slots-off JSON gains `"queue":[]`. The listing
  takes no lock and removes nothing: a ticket's liveness is a non-blocking
  try-lock released at once (a free flock is a dead waiter; a pid says
  nothing across pid namespaces), and a dead one is listed `gone`, so `N`
  equals the lines listed.

## Phases

- [x] Phase 1 — crate: ticket record, find-by-inode, `queue_in`, `promote_in`
  with its refusals and journal rows, tests: `crates/greenharness-slots/src/lib.rs`
  (also the `append_journal` holds-until comment)
- [x] Phase 2 — greengate: `slot promote`, the `slots` queue view and JSON,
  `slot run`'s id line, integration test:
  `crates/greengate-runtime/src/slot_cmd.rs`, `crates/greengate/tests/it/slot_run.rs`
- [x] Phase 3 — docs and decision: `docs/host-slots.md`, `CHANGELOG.md`
  (Unreleased), ADR-0022: a dated amendment section plus the
  `hooks.host-concurrency` `choice` text, where "Waiters are served in ticket
  order" gains the promote rule (`adr/0022-waits-end-on-a-stalled-line.md`,
  `adr/HEADS.md`), ticket Resolution
  (`docs/programmes/host-slot-admission/t13-build-reorder.md`, `map.md`
  Decisions line)

## Decision log

- 2026-11-26 — "3 moves per session per day" is a rolling 24 h window over
  the journal's `promoted` rows, because the journal is the one shared record
  and a rolling window needs no time zone. So any rotation of `waits.jsonl`
  (the t08 holds-until on `append_journal`) must keep the last 24 h of
  `promoted` rows; Phase 1 writes that into the holds-until comment.
- 2026-11-26 — a move past the age floor is refused (naming the furthest
  allowed `--to`), not clamped, because t13 asks for a refusal that names the
  limit and a silent shorter move would surprise the caller.
- 2026-11-26 — only a ticket with content can be moved, because a crate ≤ 5.1
  waiter finds itself by its cached path and would read position 0 after a
  rename (t05's required fix).
- 2026-11-26 — the row is written before the rename (panel, unix): a lost
  rename overcounts one move; a lost row would escape the budget.
- 2026-11-26 — release: greengate and `greenharness-slots` 5.2.0 (additive
  API: `Waiter::id`, `queue_in`, `promote_in`, a CLI subcommand). The driver
  cuts it.

## Verification

Each row: input → expected → observed, on tree `b93039f8`, run through
`greengate slot run` on this host. clippy `-D warnings` clean on the three
crates; `greenharness-slots` 86 passed, `greengate-runtime` `slot_cmd` 5,
`greengate` binary 7, `slot_run` integration 14.

- Phase 1: 4 waiters, 3 threads polling while the 4th (position 3) moves to
  0, and again to 1 → each reads its true place, no poll errs → observed:
  `three_waiters_polling_…_three_to_zero` and `…_three_to_one` pass (places
  `[1,2,3,0]` and `[0,2,3,1]`). The move to 1 is the one the old
  `unwrap_or(0)` fails.
- Phase 1: a ticket removed from outside → next poll errs `NotFound`; two
  tickets sharing one id → each waiter reads its own place, the promote is
  refused `ambiguous` → observed: `a_waiter_whose_ticket_is_gone_errs_not_found`
  and `a_shared_id_confuses_no_waiter_and_is_refused_as_ambiguous` pass.
- Phase 1: one refusal per limit → each names its limit, queue unchanged,
  6 `promote_refused` rows → observed: `each_limit_is_refused_by_name_and_moves_nothing`
  passes; the `budget` refusal leaves `ids` unchanged
  (`the_budget_counts_this_sessions_moves_in_the_last_24_hours`).
- Phase 1: `waits.jsonl` a directory → error, ticket keeps its place;
  A to head then B to 1 → B at 1; an empty ticket passed → place + 1; a moved
  waiter dropped → no ticket left → observed: `an_unwritable_journal_moves_nothing`,
  `a_move_to_the_head_leaves_room_for_a_move_to_one`,
  `an_old_ticket_is_passed_and_a_moved_waiter_leaves_cleanly` pass.
- Phase 1: a cut-short line, then `promoted` rows → counted, fragment not;
  no journal → first move starts it → observed: the budget test counts 2
  rows across a cut line and refuses the third move; `the_first_move_starts_the_journal` passes.
- Phase 1: the budget scan over a 14k-line journal (the step that runs under
  the lock) → under 1 s → observed: 53.7 ms (debug build, loaded host).
- Phase 2: the built binary, each process with its session variables
  removed and one set → observed: `a_session_moves_its_own_waiting_job_to_the_head`
  passes: `slots --json` lists both waiters with their sessions; another
  session's promote exits 1 `refused (own-session)`; the own session's exits 0
  with `moved … from 1 to 0, past 1 job; 2 of 3 moves left` on stderr and
  nothing on stdout; `waits.jsonl` has one `promoted` row with the reason and
  one `promote_refused` row; the waiting job printed its id and the promote
  command.
- Phase 2: the 80-column queue line, 7-digit pid and 200-character command →
  observed: `a_queue_line_fits_80_columns` passes.

## Measure

Seven days after the 5.2.0 install, from `waits.jsonl` (a follow-up ticket in
the programme): `promoted` rows per session per day (0 for 7 days means
agents do not find the command); `promote_refused` rows by `limit`; p95
`waited_ms` of wait lines against t14's baseline (map.md). Above 1.5×,
`slot promote` is turned off (refuses everything) until the person revisits
the rules.

## Implementation review

- implementation-review: approve-with-changes, round 1 then Delta (109k+65k tok, 146+65 s). Fixed: stdout→stderr, hidden queue error, one clock. Deliberate: no-room hint, `--` parse test (below).
- backend, lang:rust, tui, architect: approve-with-changes; unix approve. Round-1 fixed: typed `PromoteError`, `JobState`, one `read_queue`, flag hints out of the crate, no caller time.
- Deliberate (Delta lows): no-room hint (needs tickets 2 ns apart), session list copied in the hint, `--`/hint/lock-busy tests, `Io` cause shown twice, `safe_dir` before refusals; all as follow-ups.
- Deliberate: `JobState` exhaustive (a flock is held or free); the 1 s scan assertion kept (53.7 ms observed).
- Deliberate: the decisions.pack refresh is worktree-task's Start step, named in the PR.

## Outcome

Shipped on `fix/hsa-t13`: `greengate slot promote`, the `slots` queue view,
and the crate's find-by-inode waiter, under t06's rules, with ADR-0022
amended. Release needed: greengate and `greenharness-slots` 5.2.0 (the
driver cuts it). The off switch the Measure names shipped later in greengate
6.0.0 (t23): rollback is `git config --global greengate.hostPromote 0`
(`GREENGATE_HOST_PROMOTE=0` for one process), not a reinstall. Not shipped
here: relais's side, which waits on its lock moving to crate
5.2. Surprise: `greengate`'s top-level flag allowlist rejected `--reason`
before the subcommand saw it, caught only by the end-to-end test.

<!-- panel: repos=amont adds=contract reviewers=architect,backend,lang:rust,po,tui,unix,codex body-sha=a849198ca7b4 -->
