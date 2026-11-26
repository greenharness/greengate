+++
type = "task"
mode = "afk"
status = "closed"
blocked_by = ["t06-who-may-reorder"]
repos = ["amont", "relais"]
+++
# Build: an agent can move a job ahead

## Question

Build t05's mechanism under t06's rules. First the waiter finds its ticket by
its `-{pid}-{seq}` suffix. Then come the `queue` array in `greengate slots
--json`, and `greengate slot promote <id> --reason …` with the limits t06 sets,
journalled in `waits.jsonl`.

## Look at

- t05's and t06's Resolutions.
- `crates/greenharness-slots/src/lib.rs` (`Waiter`, `live_tickets`,
  `lock_within`), `crates/greengate-runtime/src/slot_cmd.rs`.

## Answer looks like

A merged plan and release. A test moves a waiter from position 3 to position 0
while three waiters poll, and every waiter reads its true position. A promotion
past t06's limits is refused with the limit named.

## Resolution

Built 2026-11-26 on branch `fix/hsa-t13`, plan
`docs/plans/2026-11-26-agent-moves-own-job-ahead.md` (panel: architect,
backend, lang:rust, po, tui, unix, then Codex). Ships in greengate and
`greenharness-slots` 5.2.0; the driver cuts the release.

- **Waiter.** A ticket now holds its waiter's record and `joined_ms`. The
  waiter finds its ticket by the (dev, inode) of the file it holds, not by
  name, so it reads its true place after a rename; a lost ticket is a
  `NotFound` error, never position 0. The architect seat found that
  `pid-seq` is not unique across pid namespaces, so the id is only the CLI's
  name for a job, and a promote of a shared id is refused `ambiguous`.
- **`greengate slot promote <id> --reason <text> [--to N]`.** It applies
  t06's limits under the admission lock, and each refusal names its limit
  (`own-session`, `budget`, `age-floor`, …). A move appends its `promoted`
  row first, then renames the ticket: a lost rename overcounts one move, a
  lost row would escape the budget. The budget scan splits records at each
  `{"at_ms":` start, so a cut-short best-effort line cannot hide a move.
- **`greengate slots`** lists the queue (position, id, wait, session,
  command, within 80 columns; `gone` for a dead waiter, read by a read-only
  flock that never creates a file), `--json` gains `queue`, and `slot run`
  prints its id and the promote command when it starts to wait.
- **Tests.** One move from 3 to 0 and one from 3 to 1, each while three
  waiters poll: every waiter reads its true place and no poll errs. The move
  to 1 is the one that fails on the old `unwrap_or(0)`. There is one test per
  refusal, plus an unwritable journal, a cut-short line and a 14k-line
  journal scan. The integration test runs the built binary.
- **ADR-0022** is amended (the `choice` text and a dated section).

The fog made precise:

- **How a move shows in relais's pane.** Nothing changes until relais links
  crate 5.2. Its waiters, on 5.1, write empty tickets: these can never be
  moved, and they read their place correctly when passed, as a larger
  `N ahead`. Once relais links 5.2, its own lines show the same `Behind`
  count change. Its jobs carry `session = run_id` (`verify/heavy.rs:1207`),
  not the Claude session, so an agent cannot move its relais run's job.
  Making that possible means deciding whose session a relais job belongs to,
  which is a question for t06's owner.
- **The off switch** that the plan's Measure names is not built. Rolling back
  means reinstalling 5.1.1, or a 5.2.x release that refuses every move.

