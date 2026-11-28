+++
type = "task"
mode = "afk"
status = "open"
due = "2026-11-29"
blocked_by = ["t10-held-marker-is-trusted"]
repos = ["amont", "relais"]
+++
# Build: the held marker names a live lease

## Question

Build what t10 decided, as one plan in amont with a relais slice:
- one reader in `greenharness-slots` (`held:<lease>`, a live record with that
  lease, the slot's flock taken), replacing greengate's any-non-empty test and
  relais's `== "held"`;
- writers that emit `held:<lease>`;
- a refused marker that queues with one stderr line and the journal outcome
  `marker_refused`;
- the three-step rollout (accept both forms, write the new one, refuse the
  bare one), each step released and installed before the next;
- fixtures moved to a scratch slot directory or slots off.

## Look at

- [t10's Resolution](t10-held-marker-is-trusted.md): the format, the
  readers and writers with their files, the rollout order and the fixture
  list.
- `crates/greenharness-slots/src/lib.rs`: `Lease` (`id`), `status_in_on`,
  `read_holders`.
- relais `crates/relais/src/verify/heavy.rs` (`ancestor_holds_slots`, the two
  `HOST_SLOT_ENV` inserts) and `verify/mod.rs` (`env_remove`).

## Answer looks like

Merged PRs in amont and relais, and a release of each tool, installed on this
machine. Then a check on the installed binaries:
- `GREENHARNESS_HOST_SLOT=1 amont slot run -- true` prints the refusal line
  and journals `marker_refused`;
- a command nested under a real `amont slot run` does not queue again;
- one day of `waits.jsonl` shows no `marker_refused` from a legitimate child
  (an outcome whose pid descends from a holder).

## Resolution

In progress. Phases 1 and 2 are built, merged and released; phase 3 (bare
`held` refused) runs from the day the phase-3 gate opens: 24 h after the later of the relais v0.96.0 and greengate 5.6.0 installs, and 20 accepted `held:<lease>` markers. `due` (2026-11-29) is the floor, the relais install plus 24 h. Plan:
`docs/plans/2026-11-26-held-marker-names-live-lease.md` (relais pointer plan
of the same name).

- **Phase 1, readers accept both forms.** One reader in `greenharness-slots`
  (`held_marker`, `MarkerLog`) accepts `held:<lease>` naming a live record
  whose slot is held, and the bare `held`; anything else queues with one
  stderr line and a `marker_refused` row carrying its `reason`. greengate
  #536, released in greengate 5.4.0 (#537) and installed; relais #642,
  released in relais v0.94.0 (#647) and installed.
- **Phase 2, writers emit `held:<lease>`.** greengate #542 (holders write
  `held:<lease>`, a bare read journals one `marker_bare` row) and #544 (the
  writer version is `5.6`), ship in greengate 5.6.0, which t23 is releasing beside its promote off switch; on 2026-11-28 it was not out yet (latest release 5.5.0, installed greengate 5.4.0). relais
  #649 (relais exports its own or the kept slot's lease, and passes an
  accepted inherited marker through), released in relais v0.96.0
  (#651; Release run 46131285780, every job success, brew install job included), installed with brew at 2026-11-28 10:54 CET (`at_ms` about 1795859670000) with the idle coordinator stopped first; `relais --version` reads 0.96.0. relais v0.95.0 (#650, a greentopo
  rename) sat between the two relais releases and carries none of t15.
- **Phase 3 gate.** It opens no earlier than 24 h and 20 accepted
  `held:<lease>` markers after the later of the relais v0.96.0 and greengate 5.6.0 installs, so
  not before 2026-11-29 10:54 CET; then zero `marker_bare` rows and zero `marker_refused` rows with
  reason `dead_pid` or `slot_free` (this plan's own probes excepted), and no
  holder older than that install in `greengate slots --json`. The
  plan anchored the gate on the relais install alone, but only greengate 5.6
  writes `held:<lease>` from greengate holders and journals `marker_bare`
  (relais stays on crate 5.5), so until 5.6.0 is installed the zero
  `marker_bare` condition counts nothing. If 5.6.0 is installed after
  2026-11-28 10:54 CET, `due` moves to that install plus 24 h.
- **Accepted gap.** The plan's round-1 panel rows are missing from the
  plan-review ledger: `plan-review record` found no transcript, so round 1's
  findings were applied by hand. The final-body approve (round 2) is
  recorded.
- **Follow-ups filed:** follow-ups f15
  (`slot_run::a_session_moves_its_own_waiting_job_to_the_head`, on #536),
  f16 (`rehearsal::stop_cancels_the_worker_and_removes_its_snapshot`, in the
  phase-2 commit gate), and f14
  (`cluster_gates::concurrent_gates_each_get_their_own_job`) seen again on
  #542.
