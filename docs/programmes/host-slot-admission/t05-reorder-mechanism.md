+++
type = "research"
mode = "afk"
status = "closed"
blocked_by = []
repos = ["amont", "relais"]
+++
# How the queue could be reordered safely

## Question

The queue's order today comes from ticket files under `queue/`. What does it
take to move one job ahead of others atomically, without breaking the
admission lock, a waiter's position reading, a relais run's coordinator, or a
stale ticket's cleanup? Which identity names a job (pid, session, ticket file),
so an agent can say "this one" from another process?

## Look at

- `crates/greenharness-slots/src/lib.rs`: `Waiter::new`, ticket naming,
  `live_tickets`, the `admission` lock, `holders/`.
- relais's use of the crate and its coordinator queue
  (`/Users/fredericrous/Developer/Perso/relais`).
- `greengate slots --json`: what it shows a caller today.

## Answer looks like

The data change (for example a priority field in the ticket, or a rename of the
ticket file) with its atomicity argument, the CLI it implies (`greengate slot
…`), and what each existing reader does with a reordered queue.

## Resolution

Research run 2026-11-22 (Explore agent over `greenharness-slots`, `slot_cmd.rs`,
relais `verify/heavy.rs`, and the live queue directory).

**How order works today.** A waiter's ticket is an empty file,
`queue/{nanos:024}-{pid}-{seq}`. It is created under a `.new-` name, flocked, and
then renamed into place, so a visible ticket is always locked. `live_tickets`
sorts the names, and that order is arrival order. It also deletes tickets whose
lock is free. `poll_with` reads a waiter's position under the `admission`
try-lock, and only position 0 is admitted.

**Smallest atomic change.** Move a ticket by renaming it within `queue/` to a
name that sorts earlier: the target's nanos minus 1, keeping the `-{pid}-{seq}`
suffix. Do it while holding the `admission` lock, taken with `lock_within` (a
blocking take with a deadline). `rename(2)` within one directory is atomic, the
flock follows the inode, and every reader of the queue reads it under the same
lock or returns `Waiting` without reading it.

**Required fix before any reorder.** `Waiter` caches its ticket path. After a
rename, `position` falls back to `unwrap_or(0)`, so a moved waiter would think
it is at the head. The waiter must find its ticket by its `-{pid}-{seq}` suffix,
treat a missing ticket as an error, and remove the ticket by suffix.

**Identity and CLI.** A job is named by `{pid}-{seq}`. A waiter's session and
command are on disk only once it is admitted. The optional step is to write the
`Holder` JSON into the ticket before the rename. `greengate slots --json` gains
a `queue` array (`id`, `position`, `session`, `command`, `waited_ms`). `greengate
slot promote <id> --reason <text> [--to <n>]` renames the ticket under the lock
and appends a row to `waits.jsonl`.

**What each reader sees.** greengate waiters see a smaller or larger `Behind`,
once the suffix fix is in. The `greengate slots` count is unaffected. relais
reorders only its host-slot stage: its coordinator queue is separate and was not
read. [t06](t06-who-may-reorder.md) looks at it before deciding who may jump.
