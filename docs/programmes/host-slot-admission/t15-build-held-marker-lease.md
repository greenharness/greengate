+++
type = "task"
mode = "afk"
status = "open"
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
