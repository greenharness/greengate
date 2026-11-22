+++
type = "research"
mode = "afk"
status = "open"
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
