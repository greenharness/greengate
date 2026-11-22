+++
type = "grilling"
mode = "hitl"
status = "closed"
blocked_by = ["t05-reorder-mechanism"]
repos = ["amont", "relais"]
+++
# Who may move a job ahead, and how far

## Question

With the mechanism from t05: who may reorder (any agent, only the job's own
session, the person), how far (to the head, past another session's job, past a
job already waiting longer than a bound), what reason must be given and where it
is journalled, and what stops every agent from always jumping (a budget per
session per day, a cap on jumps per job, an age floor that no jump passes)?

## Resolution

The person decided this on 2026-11-22.

- **Who:** an agent may move only its own session's jobs (the waiter's
  `session` must equal the caller's).
- **How far:** up to the head.
- **Reason:** required, and journalled as a `promoted` row in `waits.jsonl`.
- **Limits:**
  - a small number of moves per session per day; the build sets the number,
    starting at 3;
  - no move passes a job that has waited longer than 20 minutes (an age floor),
    so that a long waiter is never starved.
- **Refusals:** a refused move names the limit it hit.
- **relais's coordinator queue:** stays as it is. Only the host-slot stage is
  reordered, as t05 found.
- **Moving another session's job,** for example the person's own land ahead of
  a background run: not allowed. A session can only advance its own work.
