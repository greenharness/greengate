---
id: ADR-0022
status: accepted
decisions:
  - key: hooks.host-concurrency
    scope: hook-binary
    choice: "Every heavy job on the host (greengate's gates, relais's verification commands, and any command run through `greengate slot run`) takes host slots through one std-only crate, `greenharness-slots`. The crate counts physical cores (macOS `hw.physicalcpu`, Linux sysfs `core_cpus_list`, then `/proc/cpuinfo`), never more than the logical count, and that one count sizes the cap, the width and the load test. It also resolves the two host keys for every client, `greengate.hostCores` (1..=1024) and `greengate.hostSlots` (0..=64), from `GREENGATE_HOST_CORES` / `GREENGATE_HOST_SLOTS`, then the last `--global` or `--system` row, then the detected default, returning warnings for the caller to print. Automatic sizing (`greengate.hostSlots` unset): at most max(1, cores/2) slots; a further slot is taken only while the 1-minute load is below `cores + 4 × Σjobs(holders)`, Σjobs counting each lease's `jobs` once and `parallelism(1, cores, cap)` for a held slot with no record or a record naming none; when no slot is held, the first waiter is always admitted, whatever its weight; at most one new holder every 60 s. Waiters are served in ticket order, under one admission lock taken without blocking inside each caller's own deadline. A wait ends when its line has stalled (no admission anywhere and no change of the waiter's place for 300 s, the waiter's own marks on a monotonic clock; `GREENHARNESS_SLOT_STALL_S` is a test seam), at its caller's hard ceiling, or when a poll fails after the queue was joined; nothing runs outside the queue because a wait ended, and the journal line says which in `end`. Skipping the queue is a choice made up front and journalled `now`: `greengate slot run --now`, and `GREENGATE_ON_BUSY=now` for one run's hook checks. `greengate slot run` exits 75 at the end of a wait, its stall window never above `--max-wait`; `--on-timeout run` is refused. A holder records who it is in `holders/`. Only the process that took a slot holds it: slot descriptors are close-on-exec. `greengate.hostSlots N` fixes the count with no load test, and `0` turns slots off"
    replaces: [ADR-0021]
    reason: "One holder at `jobs 2` drove the load to 12.3 by itself on a 6-core host, so a load test against the cores alone kept slots 2 and 3 shut while it ran (2026-11-22: 1 of 3 held, 5 waiting); and a budget that counted all of a wait's time in line timed out jobs whose line was moving, after which 38 checks ran unqueued in a week, some at load 400 to 563"
  - key: hooks.busy-policy
    scope: hook-binary
    choice: "`set onBusy wait|defer` in greengate.conf (or `greengate.onBusy` in git config), default `wait`: a Heavy check waits while its line moves, up to `greengate.timeout` (3600 s when 0), and defers to CI when the line stalls for 300 s or the timeout runs out; under `defer`, a Heavy check whose slot does not free within `set slotWait` (default 60 s) while this greengate holds no slot defers, and one whose sibling in this process holds a slot waits as `wait` does. A heavy `warn` check of land's pre-stage waits while its line moves, up to `greengate.warnSlotWait` (600 s, never above the timeout), then defers. A deferred check is not run, never blocking, no stamp, never attested, so CI runs it; the push twin of a gate deferred at commit defers too. `GREENGATE_ON_BUSY` (`wait`, `defer`, or `now`, which runs every heavy check of that run unqueued at once, journalled `now`) overrides the file for one run; `now` is refused in the file and in git config, so no committed setting skips the queue"
    replaces: [ADR-0013]
    reason: "The person decided on 2026-11-22 (host-slot-admission t03) that a stalled check defers to CI and that not waiting is an explicit up-front choice: the unqueued fallback of ADR-0009 and ADR-0013 added load to a host that was already jammed"
  - key: hooks.gates-before-connection
    scope: hook-binary
    choice: "A heavy push gate — a scoped pre-push gate the pushed range touches (`dispatch::scoped_push_gates`) — never runs while git holds the remote's connection open, unless the machine asks for it. `amont.pushGates` is `refuse` by default: the pre-push hook asks one shared decision (`decide`: a push stamp or cluster token on every tip, a recorded non-pass completion, a commit-time pair gated or deferred, a twin proved on the cluster) for each owed gate, and if any would have to run it prints the gates and the commands that run them first (`amont land`, `amont rehearse --wait`; `git config amont.pushStamps true` instead when stamp reuse is off) to stderr and exits 1 before any gate starts, without waiting for a background rehearsal. `inline` runs them in the push, as amont did before 2.0. `amont land` runs a pre-stage before every `git push` attempt: it fetches a remote tip the clone lacks or refuses, refuses when stamp reuse is off and gates are owed, and otherwise runs a child `amont rehearse --wait` told the push's real ref line (`AMONT_LAND_PRESTAGE`), whose stdout goes to land's stderr; a failing gate or a pre-stage that cannot run refuses with nothing pushed, a signal ends land `interrupted`. Its push carries `AMONT_LAND=<sha> <remote tip>`, read once and removed; that hook refuses anything still owed, printing `amont-land: remote-moved` when the range's remote side is not the pre-stage's and `amont-land: evidence-missing` otherwise, and land matches those sentinels. Gates that completed without passing (a warning, an unavailable tool, a deferral on a busy host) are recorded per tree in `refs/notes/amont-gate-done`, never as a stamp or an attestation, written under the same clean-content condition as push stamps and only with stamp reuse on. A completion record names how the gate ended and answers only while it should: a warning for good, a failure under `warn` severity while the gate is still `warn`, an unavailable tool for an hour, a gate deferred because its wait for a host slot ended (ADR-0022: in a rehearsal or land's pre-stage too) for an hour, so land's push does not refuse it and CI runs it. A rehearsal never cancels land's pre-stage (recorded as `kind=prestage` in its state) and the pre-stage never cancels a rehearsal: either waits for the other, for at most three hours. The dependency audits leave the connection too: wherever one runs on the pushed content its report is recorded in `refs/notes/amont-audit` (local, never pushed), keyed by the lockfiles' blobs for a branch push and by the tree for a `v*` tag, and a push with a fresh record (`amont.auditMaxAge`, 24 h; one hour for a report that could not check) replays it against today's waivers and the push's stakes; otherwise the audit is owed like a heavy gate. `amont land --tag <name>` pre-stages and pushes one tag, refusing one already on the remote, and a refused bare tag push names it. An unwalkable push range counts every tracked file as changed. Deletions owe nothing; rehearsal snapshots never refuse"
    replaces: [ADR-0018]
    reason: "A gate that defers in land's pre-stage records nothing, so land's own push would refuse it as missing evidence: deferring to CI must leave the push able to reach CI"
---
# 0022 — a stalled wait defers; admission counts the holders

Plan: [`docs/plans/2026-11-23-admission-counts-holders-stalled-waits.md`](../docs/plans/2026-11-23-admission-counts-holders-stalled-waits.md).
Programme: `docs/programmes/host-slot-admission` (tickets t01, t02, t03).

Replaces ADR-0021's `hooks.host-concurrency`, ADR-0013's `hooks.busy-policy`
and ADR-0018's `hooks.gates-before-connection`. Each keeps everything its
record decided except the clauses named below. It amends the lineage that
began with ADR-0009 ("a slot that never frees runs the check unqueued with a
note"), which ADR-0013 had carried on as `onBusy wait`. It takes effect with
the greengate 4.14.0 install.

## Context

On 2026-11-22 `greengate slots` showed 1 of 3 slots held, load 12.3, 5
waiting. The one holder, greengate's integration suite, kept the load above
the core count by itself, so the load test (`load1 < cores`) never opened
slots 2 and 3. The journal since 2026-11-21 18:00 held 38 timeouts, 50
deferrals and 29 unqueued runs, some started at load 445 to 563.

## Decision

- **Admission counts the holders.** A further slot is taken while
  `load1 < cores + 4 × Σjobs(holders)`. The 4 is t01's measurement: one
  holder at `jobs 2` measured 12.3 against an idle baseline of about 4, so
  (12.3 − 4) / 2 ≈ 4 per job. One holder at `jobs 2` gives a limit of 14 on
  6 cores, which admits the second job. Outside load of 300 with two holders
  gives 22, which still holds it back. The floor, the cap and the ramp stay.
- **A wait ends on a stalled line.** A wait ends after 300 s with no
  admission anywhere and no change of the waiter's place. A budget no longer
  ends a wait while the line moves; the caller's hard ceiling still ends it.
  `slot run`'s `--max-wait` stays hard, because the Bash tool kills a command
  at 120 s, and `onBusy defer`'s `slotWait` stays hard, because a deferral is
  the repository's own choice.
- **Nothing runs unqueued because a wait ended.** A hook check defers to CI,
  and `slot run` exits 75. In a rehearsal or land's pre-stage, a deferral is
  recorded as an hour-long completion, so land still pushes and CI runs the
  gate.
- **Skipping the queue is said up front.** `slot run --now`, or
  `GREENGATE_ON_BUSY=now` for one run, is journalled `now`. It is never set
  from a committed file.
- **relais** applies the same stall rule to its verification commands. Its
  own budget record, relais's `execution.resource-budgets`, is amended in the
  relais repository.

## Consequences

- On a jammed host, a commit's heavy checks, and a land's pre-stage gates,
  defer to CI where they used to run unqueued after the timeout. The
  deferral roll-up offers the command that waits first, then
  `GREENGATE_ON_BUSY=now`; it offers `set onBusy wait` only after a `slotWait`
  deferral, because a stall defers under either policy.
- `waits.jsonl` gains `end` (`stalled`, `ceiling`, `io_error`) and the `now`
  outcome, and `holder_jobs` counts a record with no `jobs` as one slot's
  width, as the load test does.
- A `deferred` completion is read by greengate 4.14 and later. An older
  binary does not know the word, so its push refuses the gate as missing,
  which fails closed.
- Rollback: reinstall greengate 4.13, and note it here. relais can stay on
  the 4.14 crate.
