+++
type = "task"
mode = "afk"
status = "closed"
blocked_by = ["t12-build-relais-setup-lease"]
repos = ["relais", "amont"]
+++
# Can an agent move its own relais job ahead?

## Question

relais links `greenharness-slots` 5.0/5.1. Its waiters write empty queue
tickets, which `slot promote` can never move and which read one more job
ahead when a job passes them. Even after a bump to crate 5.2, relais tags its
jobs with its run id, not the Claude session that started the run, so an
agent cannot move its own relais job under t06's own-session rule. Two
questions: does relais bump to crate 5.2 so its tickets carry an owner, and
whose session does a relais job belong to (the run's, or the Claude session
that dispatched it)? The answer also decides how a move shows in relais's
pane. It waits for [t12](t12-build-relais-setup-lease.md), which changes the
same relais slot code (`verify/heavy.rs`, `verify/mod.rs`).

## Look at

- [t06](t06-who-may-reorder.md) (own session only) and
  [t13](t13-build-reorder.md)'s Resolution.
- relais `verify/heavy.rs` (`slots::take`, the session it passes) and the
  pane code that lists queued runs.
- `crates/greenharness-slots/src/reorder.rs`: what a ticket records.

## Answer looks like

A decision on the session a relais job belongs to, with its reason, then a
merged and released relais change on crate 5.2 whose test moves a relais
waiter ahead from its owning session, or a recorded reason to leave relais
jobs unmovable.

## Resolution

Decided 2026-11-28 by the ticket agent (ADR-0080); reopen: set
`status = "open"` in this file.

- **The crate bump is already done.** relais links `greenharness-slots`
  5.5 since t15 (`crates/relais/Cargo.toml`), so its tickets carry their
  holder and are found by inode: a relais waiter can be moved and reads its
  true place after a move.
- **A relais job belongs to the Claude session that dispatched the run.**
  t06 lets a session advance its own work, a run is that session's work,
  and a run id is never the identity of a `greengate slot promote` caller,
  so tagging by run made every relais job unmovable by construction.
  relais's holder (`verify/heavy.rs`, `slots::holder`) now takes the run's
  `session_id`: `RELAIS_SESSION_ID`, which the plugin sets to the Claude
  session id (`claude-plugin/hooks/runs.ts`), else `CLAUDE_CODE_SESSION_ID`.
  That is the id the crate's `session_id()` reads in the same session's
  Bash, so the owner's promote matches and another session's is refused
  `own-session`.
- **Known limits.** With no session, or only relais's
  `unattributed-ppid-N` stand-in, the holder keeps the run id: nobody can
  move such a job, and the run id tells those rows apart where 8 characters
  of `unattrib` would not. The two tools read the session variables in
  opposite orders (relais `RELAIS_SESSION_ID` first, the crate
  `CLAUDE_CODE_SESSION_ID` first); a caller who sets `RELAIS_SESSION_ID` to
  something other than its Claude session gets a job it cannot move.
  `/clear` starts a new session, so a run dispatched before it can no
  longer be moved from after it.
- **How a move shows in the pane.** No pane change: relais emits a
  `waiting_for_slot` event with `position` at each announce, so after a move
  the pane reads the new place (2 → 0 in the test). `greengate slots` and
  the `released` rows of `waits.jsonl` show the session for relais jobs.
- **Test.** `the_dispatching_session_moves_its_relais_job_ahead_and_no_other_can`
  (relais `verify/heavy/tests.rs`): two waiters of another session queue
  ahead of a relais command; from inside the relais wait a promote as the
  other session is refused `own-session`, the promote as the dispatching
  session moves it from 2 to 0, and the relais command is granted while
  both others still wait. `a_holder_with_no_real_session_is_named_by_its_run`
  pins the fallback.
- **Change:** relais PR fredericrous/relais#653 (merged 1fd3d34f), plan
  `docs/plans/2026-11-28-relais-jobs-belong-to-session.md` (panel: backend,
  lang:rust, tui, unix; implementation review and deltas approve). Not yet
  released: the relais release and install are the driver's, and the live
  check (session prefix in `greengate slots`, a promote from the dispatching
  session) is owed after that install.

Follow-up: relais could print a waiting job's id and the promote command
when it starts to wait, as `slot run` does; until then an agent finds it in
`greengate slots` by the `relais` command and its session's first 8
characters.
