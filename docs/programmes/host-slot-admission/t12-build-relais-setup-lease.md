+++
type = "task"
mode = "afk"
status = "closed"
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

## Resolution

Built 2026-11-26 in relais (plan
`docs/plans/2026-11-26-setup-keeps-one-host-slot.md`, relais#624), released in
relais v0.90.0 (release #628, Release run 45890258832: conclusion success,
with the homebrew tap published and `brew can install what was published`
success), installed on this machine 2026-11-26 21:28 CET (`relais --version`
printed `relais 0.90.0`).

- `run_setup_gated_within` opens a kept slot (`HeavyAdmission::keep_slot`,
  a guard). The first step that queues for host slots takes them through
  `slots::take` and stores the `Lease` in the kept slot. Each later step of
  the same weight is relabelled with `Lease::rename`, takes no new `Waiter`,
  writes no wait row and charges no admission wait. It runs under the held
  marker, and emits `host_slot_kept`. The guard's drop writes the one
  `released` row.
- Lock order: the run never waits on relais's coordinator while it keeps
  the slot, because the coordinator's holders wait for slots while holding
  its tokens. A step the coordinator queues, holds for the disk, cannot be
  reached for, or has forgotten the run for gives the slot back first and
  then takes one afresh. So does a disk short after the grant, an unusable
  slot directory, or a step of another weight. Each give-back emits
  `kept_slot_given_back` with its reason. A setup the coordinator contends
  still re-queues per step.
- Test: `verify::heavy::tests::setup_keeps_one_slot`. With 3 steps and 1
  slot, held by another tool for the first 3 s, the setup makes one
  `slots::take` (one relais row with `waited_ms`) and writes one `released`
  row. The admission budget is spent once (3 s, all in step 1), and a waiter
  that joins after step 0 stays `Waiting` through every step. With the guard
  line removed, the test fails. Further tests cover the coordinator queue,
  an unreachable coordinator, a weight change, a disk short, and the
  failed and cancelled setup.
- Measured before (`waits.jsonl`, 24 relais setups): a whole setup holds
  p50 0.8 s, p95 419 s, max 425 s. 2 of 24 hold past the 300 s stall
  window, so a waiter behind them sees no admission for that long: accepted,
  as one long holder.
- Held marker: still the bare `held`. It is now written from one constant
  (`HELD_MARKER`, `verify/heavy.rs`) at both write sites. The reader
  `ancestor_holds_slots` (`== "held"`) is unchanged. t15 replaces the
  constant with `held:<lease>`. A kept slot keeps one lease id for the whole
  setup, so the per-lease form fits as is.
- Gap: a signal kills the runner without the drop. The kernel frees the
  flock, but no `released` row is written and the holder record keeps the
  last step's label. This now spans the whole setup, not one step.
- Not yet observed on the installed binary: a live 2-step setup (one relais
  `admitted` row and one `released` row with `held_ms` spanning both steps)
  and a `kill -TERM` mid-setup. relais's own profile has one setup step, and
  a scratch-repo run needs a trust grant. These go to t16's journal count,
  which reads relais setup rows from the installed binary.
- CI found what local macOS did not: the windows job needed `#[cfg(unix)]`
  on the test module, and ubuntu polled 5 times where macOS polled 3. The
  test now asserts that no sleep happens after step 0, not an exact count.
