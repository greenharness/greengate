+++
type = "research"
mode = "afk"
status = "open"
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
