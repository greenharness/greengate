+++
type = "task"
mode = "afk"
status = "open"
blocked_by = ["t09-journal-holders-and-hold-time"]
repos = ["relais"]
+++
# Build: one host slot lease for a relais setup

## Question

Build t07's choice in relais. `run_setup_gated_within` takes one lease, passes
it to each step, relabels it per step with `Lease::rename`, and exports the held
marker so that no step queues again.

## Look at

- t07's Resolution and its two test seams.
- relais `verify/mod.rs` (`run_setup_gated_within`), `verify/heavy.rs`
  (`admit`, `slots::take`).

## Answer looks like

A merged and released relais change, with the test showing one `slots::take`
for N setup steps, and one budget spend for `admission_wait_exhausted`.
