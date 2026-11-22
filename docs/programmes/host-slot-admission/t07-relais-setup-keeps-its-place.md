+++
type = "research"
mode = "afk"
status = "closed"
blocked_by = []
repos = ["relais"]
+++
# A relais run keeps its place between setup steps

## Question

A relais run takes a slot per setup step (`yarn install`, `postinstall`, …) and
goes back in line between steps, so under load it starves: on 2026-11-21 three
submissions in a row ended `admission_wait_exhausted` at 1800 s while greengate
gates held the slots. Should a run hold one lease for its whole setup, re-enter
at the head after its first admission, or ask for a higher weight once?

## Look at

- relais: setup-step execution and its slot acquisition, and the
  `admission_wait_exhausted` path (`/Users/fredericrous/Developer/Perso/relais`).
- memory `slots_physical_cores_plan_2026_11_10` (the 2026-11-21 observation).
- `crates/greenharness-slots/src/lib.rs`: whether a lease can span several
  commands today (`held_in_process`, `own_held`).

## Answer looks like

The chosen option with its effect on the other waiters (how long a whole setup
holds a slot, from relais's own run logs), and the seam where a test reproduces
the starvation.

## Resolution

Research run 2026-11-22 (Explore agent over relais `verify/` and `runner/`,
`greenharness-slots`, and the 744 relais rows in `waits.jsonl`).

**Chosen: one lease for the whole setup.** Today `run_setup_gated_within`
(`verify/mod.rs`) runs each setup command through `admit()` (`verify/heavy.rs`).
Each `admit()` makes a new `Waiter`, which joins at the tail of the queue and
drops its lease when the command ends. The `Lease` type is not tied to a single
command, and `Lease::rename` already relabels a holder. So the setup takes one
lease, passes it to every step, and exports the held marker so that no step
queues again.

**Measured.** 8 recent relais runs timed out at 1,800 s without ever being
admitted. Runs that were admitted waited again at their next step:

| pid | first wait | next wait |
|---|---|---|
| 4761 | 705 s | 966 s |
| 12557 | 128 s | 982 s |
| 91012 | 133 s | 607 s |

How long a whole setup holds a slot is not in the journal; that is
[t09](t09-journal-holders-and-hold-time.md).

**Rejected.** Re-entering at the head after the first admission is a queue jump,
which is [t06](t06-who-may-reorder.md)'s question, not a default. A higher weight
waits for more free slots at once, and takes slots the setup does not use.

**Effect on other waiters.** A setup now holds one slot for the sum of its
steps, the same as one long gate does, instead of competing at every step.

**Test seams.** In the crate: grant a waiter, drop it, add a fresh waiter, and
assert the fresh one is `Behind` a queued ticket. That is the starvation in
miniature. In relais: drive `run_setup_gated_within` with a fake gate (like the
test `a_setup_step_runs_before_the_commands…`) and assert one `slots::take` for
N steps.
