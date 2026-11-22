+++
type = "research"
mode = "afk"
status = "closed"
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

## Resolution

Research run 2026-11-22 (Explore agent over `host_slots.rs`, `slot_cmd.rs`,
`lib.rs`, relais, and `waits.jsonl`).

**Rule, for both tools.** A wait ends when the line has made no progress for
`N = 300 s`. The idle clock is `now − max(enqueue time, admitted_at, my last
position change)`. The existing budgets (`greengate.timeout`, relais's
`admission_wait_seconds`) stay as a hard ceiling against starvation at the
back, but they no longer end a wait while the line moves. A job at the head that
is held back only by the load test counts as stalled, which is intended.
`Queue` gains `last_admitted_ms` from `admitted_at`, which `poll_with` already
reads.

**Tally, 38 timeouts since 2026-11-21 18:00.** In 15 of them the line moved: at
least one admission happened during the wait. In the other 23 nothing was
admitted at all. With `N = 300 s`, 14 of the 38 would have been saved; with
`N = 600 s`, 31. The 90 s `slot run` default would still time out under either
value. `slot run` also lives inside Claude Code's 120 s Bash limit, so the
answer there is a detached wait, not a longer one.

**Where it lives.** greengate: `wait_for_slot` (`host_slots.rs`, stops at
`started.elapsed() >= budget`) and `slot_cmd.rs` (`max_wait`, exit 75). relais:
`policy/mod.rs` (`admission_wait_seconds`), `runner/engine.rs`
(`Exhausted::AdmissionWait` → `admission_wait_exhausted`) and its wait loop in
`runner/launch.rs`. That loop was not read, so the build reads it first.

**Test seam.** `Waiter::poll_with` already takes the clock in `Probe`.
`wait_for_slot` reads `Instant::now()` and needs a clock argument. The red test:
a waiter at position 10, one admission every 60 s, a 90 s budget; it must not
time out after 400 s. Its pair: no admission for `N` must time out.

**Risks.** Any admission resets the clock, including one for an unrelated
session. The `admitted-at` file is machine-wide, so both tools must read it the
same way. Before the semantics change, resolve ADR-0009 and ADR-0013 with aval.
