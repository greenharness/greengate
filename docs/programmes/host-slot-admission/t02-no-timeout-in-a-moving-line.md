+++
type = "research"
mode = "afk"
status = "open"
blocked_by = []
repos = ["amont", "relais"]
+++
# A wait times out only when the line stops moving

## Question

A wait's budget counts all its time in line: `greengate slot run` (90 s by
default, `--max-wait`), a hook check's `greengate.timeout` ceiling, relais's
`admission_wait_exhausted` (1800 s). A job ten places back in a healthy queue
times out although nothing is wrong. What should end a wait instead: no
admission anywhere for N minutes (a stalled queue), a budget that starts when
the job reaches the head, or a budget scaled by the work ahead of it?

## Look at

- `crates/greengate-runtime/src/host_slots.rs`: `budget`, `ceiling`,
  `BusyPolicy`, `wait_for_slot`; `crates/greengate-runtime/src/slot_cmd.rs`
  (`--max-wait`, exit 75).
- `crates/greenharness-slots/src/lib.rs`: `admitted_at`, `live_tickets`, how a
  waiter knows its position and the last admission.
- relais: where `admission_wait_exhausted` is raised and its budget
  (`/Users/fredericrous/Developer/Perso/relais`).
- `waits.jsonl`: the 38 `timeout` rows since 2026-11-21 18:00, and whether an
  admission happened during each wait.

## Answer looks like

The rule that ends a wait, stated once for both tools; for each timeout row in
the journal, whether the new rule would still have timed it out; and the test at
the `wait_for_slot` seam that goes red when a job in a moving line times out.
