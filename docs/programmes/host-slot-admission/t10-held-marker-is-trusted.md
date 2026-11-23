+++
type = "research"
mode = "afk"
status = "closed"
blocked_by = []
repos = ["amont", "greenbelay"]
+++
# A hand-set held marker skips the queue

## Question

`GREENHARNESS_HOST_SLOT` with any non-empty value makes `slot run` and the hooks
treat the process as already holding a slot (`host_slots.rs`, `slot_cmd.rs`).
A child of a real holder needs that. But `GREENHARNESS_HOST_SLOT=1 cargo test`,
typed by hand, also skips the queue, and the greenbelay rule stays silent
whenever the variable is set. Should the marker carry something only a real
holder can give, such as the holder's slot file and pid, checked to be alive?
And how often is it set by hand?

## Look at

- `crates/greenharness-slots/src/lib.rs` (`HELD_ENV`), `host_slots.rs`
  (`nested` test), `slot_cmd.rs` (`Launch::Held`, the export to children).
- `greenbelay-guard/src/rules/heavy_build_unqueued.rs` (the `confirm` docs).
- Claude transcripts under `~/.claude/projects/` from the last 7 days, scanned
  for `GREENHARNESS_HOST_SLOT=` in Bash commands (command strings only).

## Answer looks like

The count of hand-set uses; a marker format a real holder writes and a reader
checks (the slot it names is locked, and its holder pid is an ancestor or
alive), with the cost of the check; and what a stale or forged marker does: it
queues, with one line saying why.

## Resolution

**Count: zero hand-set uses.** Over 7 days (to 2026-11-23), 2,404 Claude
transcripts held 20 Bash commands with `GREENHARNESS_HOST_SLOT=`. None sets it
to skip the queue: 13 are commit, CHANGELOG and PR text, 2 are greenbelay bench
scripts (a test fixture), 3 are this programme's own charting, and 2 are this
scan. The relais state (`~/.local/state/relais`: the hook journal and the run
patches) holds only documentation text. `relais.toml` names the marker in a
comment only. No config, CI workflow or dotfile sets it.

**The ticket's premise, corrected.** The greenbelay rule reads the hook
process's environment, not the command (`heavy_build_unqueued.rs`, `confirm`).
Probed with the installed binary, `GREENHARNESS_HOST_SLOT=1 cargo test
--workspace` is still advised. That command never reads the marker, so it is
simply unqueued, which the rule already catches. The real hole is an inline
marker on a command that reads it. The rule was silent on all three of these:
- `GREENHARNESS_HOST_SLOT=held amont slot run -- cargo test --workspace`
- `GREENHARNESS_HOST_SLOT=1 git commit -m x` (the greengate hooks run nested)
- `GREENHARNESS_HOST_SLOT=held amont run pre-push-cargo-test`

The rule treats these commands as queued and never judges them. Each skips the
queue.

**Three readers, three rules today.**

| Reader | Where | What it accepts |
|---|---|---|
| greengate | `host_slots.rs` `nested`, `slot_cmd.rs` | any non-empty value |
| relais | `verify/heavy.rs` `ancestor_holds_slots` | exactly `held` |
| greenbelay | `confirm` | the variable's presence, even when empty |

So `=1` skips greengate's queue but not relais's.

**Marker format.** A real holder writes `held:<lease>`, where `<lease>` is
the lease id (`<pid>-<seq>`). The lease already writes that id into every
`holders/slot-N.who` record, and `Lease::rename` keeps it. A child of the
holder inherits the marker unchanged.

The check is one shared reader in `greenharness-slots`, called by greengate
and relais. It returns `Held` only when both of these hold:
1. A `holders/slot-N.who` record carries that lease id, and its pid is alive.
2. That slot's flock is taken. This is probed under the admission lock, as
   `status_in_on` does. When the admission lock is busy, the record and a live
   pid suffice, as `status` already allows.

Ancestry is not required. relais's coordinator keeps a process group's
tokens without being its ancestor, and a liveness check is enough against
the threat that matters.

**Threat model.** The check stops careless or habitual agents, not a
determined one. The records are 0644, so a lease id can be copied from a
live holder. Nothing secret is added, deliberately.

greenbelay keeps its presence check of the hook's own environment. That
environment holds the marker only when the whole Claude session runs under a
holder, which is legitimate. Its module doc says it never reads slot files,
and that stays true.

**Cost.** The in-process check is a few hundred bytes read from up to 3 files,
one `kill(pid, 0)` and up to 4 non-blocking flocks: well under a millisecond.
The upper bound measured on this host: `greengate slots --json`, which does
the same probe plus a process exec, took 30 ms median and 38 ms p95 over 20
runs at load 6–15.

**A stale or forged marker** (bare `held`, `1`, a dead or unknown lease)
queues like any unmarked command, with one stderr line:

```
greengate: GREENHARNESS_HOST_SLOT=<value> names no live lease; queueing
```

The journal records it as the outcome `marker_refused`. The wait it then
takes ends under t02's stall rule and t03's fallback, never an unbounded wait,
so a legitimate child that fails the check costs at most one stalled wait. It
never deadlocks behind its own ancestor.

**Rollout order** (old and new binaries share the queue):
1. Every reader accepts both bare `held` and `held:<lease>`. A new writer
   under an old relais (which reads exactly `held`) would otherwise queue
   again for a slot its own ancestor holds.
2. Writers emit `held:<lease>`: `slot_cmd.rs` `Launch::Held`,
   `hooks/common.rs` (both sites) and relais `heavy.rs` (both sites).
3. Once both tools are installed on this machine, readers refuse anything
   but a live `held:<lease>`.

**Test fixtures that set a bare marker** would read as forged under step 3,
and would queue behind the suite running them:
- greengate: `tests/common/mod.rs` `pin_host_env`, `tests/it/slot_run.rs:191`
  and `tests/timing.rs:553` (sets a path).
- greenbelay: `tests/hook.rs` `HostSlot::Held`. This only feeds its presence
  check, so it is unaffected.
- relais: the `verify/heavy` tests.

Their escape is a scratch slot directory (`GREENHARNESS_HOST_SLOTS_DIR`) or
slots off (`GREENGATE_HOST_SLOTS=0`), never a bare marker.

**The build** is filed as [t15](t15-build-held-marker-lease.md).

