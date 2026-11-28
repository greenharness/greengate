# Host slots

The contract of the `greenharness-slots` crate (greengate ADR-0014, decisions
ADR-0049). A test suite, a build or a type checker already uses every core
on its own, so a heavy job takes a host slot before it runs. Every heavy job
on the machine takes its slot through this one queue: greengate's git-hook gates,
relais's verification commands, and any command run through
`greengate slot run`. relais links the crate; it does not carry a copy.

## The directory

`GREENHARNESS_SLOT_DIR`, else `$XDG_RUNTIME_DIR/greenharness-slots` on Linux, else
`/tmp/greenharness-slots-<uid>`. It is a fixed path, never `$TMPDIR`, which macOS
and agent sandboxes set per session. It must be a real directory owned by
this user with mode 0700. Otherwise slots are off for the job, and it says
why.

| Path | What it is |
|---|---|
| `slot-0` … `slot-{cap-1}` | the slots. A slot is held while a process holds `flock(LOCK_EX)` on its file. |
| `admission` | the one admission lock. Every admission decision is made under it, taken with `LOCK_NB` and retried inside the caller's own deadline, so a stopped holder of it never holds a hook past its own deadline. |
| `admitted-at` | the time of the last admission, in ms since the epoch, for the ramp. |
| `queue/<nanos>-<pid>-<n>` | a waiter's ticket, flocked for as long as it waits. It is created hidden (`.new-…`), locked, then renamed into place, so a visible ticket is never unlocked. A visible ticket whose lock is free belongs to a waiter that died, and is removed. From 5.2 it holds the waiter's holder record plus `joined_ms`, written before it is visible; an empty ticket is an older client's. A waiter finds its ticket by the (dev, inode) of the file it holds, not its name, because a [promotion](#moving-a-job-ahead) renames it. `<pid>-<n>` is the job's id. |
| `holders/slot-N.who` | who holds slot N, as a JSON object (`lease`, `pid`, `tool`, `repo`, `command`, `session`, `started_ms`, and `jobs` when the writer knows it). `lease` (`<pid>-<n>`, from 4.13) is the same in every record of one lease and survives a rename, so a reader counts a lease once. It is written after the slot is won, through a hidden temp file and a rename and removed before it is released. A reader trusts it only while `pid` is alive. A hidden `.slot-N.who.<pid>.tmp` beside it is a rename in flight; when a slot is granted, those whose `pid` is dead are removed, as dead queue tickets are. So is a `slot-N.who` whose slot's flock is free (its holder was killed, or exited through `process::exit` on a signal): the free flock, not the pid, proves the holder is gone, so a reused pid does not make it count again once a slot has been granted since. It lives in a subdirectory so an old client counting `slot-*` files never mistakes it for a slot. |
| `waits.jsonl` | one line per acquisition and, from 4.13, one per released lease; see [The journal](#the-journal). |

## The journal

`waits.jsonl` is appended by every writer, one line per write.

**Wait lines**, one per acquisition: `at_ms`, `tool`, `pid`, `weight`,
`cores`, `waited_ms`, `load1`, `outcome` (`admitted`, `deferred`,
`unqueued`, `timeout`, `handed`, and from 5.0 `now`), `jobs` and `cap`
when the writer knows them. From 5.0 a line whose wait ended without a slot
also carries `end`: `stalled`, `ceiling` or `io_error` (see
[When a wait ends](#when-a-wait-ends)); `unqueued` then means only that the
slots were unusable (off, an unsafe directory, a queue that could not be
joined). Lines written by greengate's checks also carry `name` (the check),
`session` when one is known, and `"rehearsal":true` / `"prestage":true`
inside a rehearsal snapshot / land's pre-stage.

From 4.13 every wait line also carries `holders` (slots held when the line
was written) and `holder_jobs` (the `jobs` of the distinct leases holding
them). They are read from the live holder records without a lock: an
observation, not an atomic count. Callers write the line after the grant, so
an `admitted` or `handed` line counts its own lease (subtract its `weight`
and `jobs`), while `timeout`, `unqueued` and `deferred` lines count only the
others. Both are `null` when a record could not be read. A line without them
comes from an older writer: its occupancy is unknown, not 0. From 5.0 a
record that names no `jobs` adds one slot's width (`parallelism(1)`) to
`holder_jobs`, as the load test counts it; under 4.13 it added 0.

**Release lines**, one per lease when it is dropped, from 4.13: `at_ms`,
`tool`, `pid`, `session`, `command` and `jobs` (the grant's), `weight`,
`granted_ms` (wall clock), `held_ms` (monotonic, grant to drop) and
`"outcome":"released"`. A lease greengate hands from gate to gate leaves one
release line for its `admitted` line and all its `handed` lines. A holder
that exits without unwinding (a kill, or `land` interrupted by a signal)
leaves none, so an `admitted` line that carries `holders` and has no release
is an interrupted holder, not a lost write. A release line is appended after
the slot is freed, so a successor's line can come before it: order by
`at_ms`, not by line.

**Promotion lines**, from 5.2, one per [move](#moving-a-job-ahead): `at_ms`,
`tool`, `pid`, `session`, `"outcome":"promoted"`, `id`, `from`, `to`,
`passed`, `command`, `reason`. A refused move writes one
`"outcome":"promote_refused"` line with `id`, `session` and `limit` (and `source` for `off`). Neither
has `waited_ms`. A rotation of the journal must keep the last 24 h of
`promoted` lines: the move budget is counted from them.

Wait lines only (a release line has no `waited_ms`; `-R` with `fromjson?`
skips a half-appended last line):

```sh
jq -cR 'fromjson? | select(has("waited_ms"))' /tmp/greenharness-slots-$(id -u)/waits.jsonl
```

Each release with the first `admitted` line of the same `pid` and `tool` at
or after its `granted_ms`:

```sh
jq -sR '[split("\n")[] | fromjson? ] | sort_by(.at_ms) as $rows
  | $rows[] | select(.outcome == "released") as $r
  | {release: $r, admitted: ([$rows[] | select(.outcome == "admitted"
      and .pid == $r.pid and .tool == $r.tool and .at_ms >= $r.granted_ms)][0])}' \
  /tmp/greenharness-slots-$(id -u)/waits.jsonl
```

## Admission

Only the oldest live ticket (the first by name, which a
[move](#moving-a-job-ahead) can change) may take slots. It takes `weight` slots, all or
none, under the admission lock, when:

- **no slot is held:** the floor. The first waiter on an empty host is
  always admitted, whatever its weight up to the cap;
- **`greengate.hostSlots` is unset (automatic):** a free slot exists, the
  one-minute load is below `cores + 4 × Σjobs` (ADR-0022: Σjobs is the `jobs`
  of the holders, once per lease; a held slot with no record, or a record with
  no `jobs`, counts `parallelism(1)`), and at least 60 s have passed
  since the last admission, unless the waiter's own process already holds a
  slot (one commit's checks are not spaced apart). The cap is
  `max(1, cores/2)`, with `cores` as [Cores](#cores) counts them;
- **`greengate.hostSlots N`:** N free slots are enough, with no load test and no
  ramp. `0` turns slots off.

The 60 s ramp is the load average's own lag: 10 s after a new holder starts,
the one-minute load shows only about 15% of it. The `4 × Σjobs` is the
holders' own load: one holder at `jobs 2` measured 12.3 against an idle
baseline of about 4, so a test against the cores alone kept every other slot
shut while that holder ran.

A waiter that is not admitted says why, after whom it waits behind:
`(2 ahead)`, `(slots full 3/3)`,
`(load 31.0 >= 14 (6 cores + 4×2 holder jobs))` or
`(one new holder per minute)`. The first such line is followed, once, by
`see: greengate slots`.

## Moving a job ahead

An agent session can move one of its own waiting jobs ahead, up to the head
(programme host-slot-admission t06, the person's decision; ADR-0022):

```sh
greengate slot promote <id> --reason <text> [--to N]
```

`<id>` is the job's `<pid>-<n>`, as `greengate slots` lists it and as
`slot run` prints it when it starts to wait. `--to` is the place to move to,
0 (the head) by default. Under the admission lock, the move is refused, by
name, when:

| Limit | When |
|---|---|
| `no-reason` | the reason is empty |
| `no-session` | the caller has no session id (`CLAUDE_CODE_SESSION_ID`, `RELAIS_SESSION_ID`, `CLAUDE_SESSION_ID`) |
| `not-queued`, `ambiguous` | no live ticket has the id, or two do (waiters in two pid namespaces) |
| `old-client` | the ticket is empty: an older client's, which finds itself by path |
| `own-session` | the job belongs to another session, the person's included |
| `already-ahead` | the job is at or before the place asked for |
| `budget` | the session made 3 moves in the last 24 h (rolling); the refusal says when the next frees |
| `age-floor` | the move would pass a job that has waited 20 min or more; the refusal names it and the furthest `--to` that passes none |
| `no-room` | no name sorts between the place's neighbours |
| `off` | promotion is turned off on this machine (`greengate.hostPromote 0`, or `GREENGATE_HOST_PROMOTE=0`); checked first, before the other limits, and it creates no slot directory |

Otherwise the `promoted` line is appended first, then the ticket is renamed
to sort just before the job at `--to` (1 s before the head, or the midpoint
between two neighbours), keeping its id. A failed append moves nothing; a
failed rename leaves a counted move that did not happen. Passed waiters read
a larger `Behind` and count it as a change of place for the stall rule.

Exit status: 0 moved (stderr: `moved <id> from 3 to 0, past 3 jobs; 2 of 3
moves left in 24 h`), 1 refused or slots off, 2 a usage error, 74 an I/O
failure (queue or journal), 75 the admission lock stayed held for 10 s.
Every message goes to stderr; the queue view (`slots`) is the stdout a script reads.

**Turning it off.** `git config --global greengate.hostPromote 0` refuses every
move with `refused (off)` (see
[`greengate.hostPromote`](configuration.md#greengatehostpromote--the-off-switch-for-slot-promote));
`git config --global --unset greengate.hostPromote` turns it back on.

## Cores

One count sizes the cap, the width and the load test (ADR-0021): the
physical cores, never more than the logical count, which also honours a
cgroup or affinity limit. macOS reads `hw.physicalcpu`; Linux counts the
distinct `core_cpus_list` entries in sysfs, then the distinct
(`physical id`, `core id`) pairs in `/proc/cpuinfo`, and falls back to the
logical count where neither says (aarch64 VMs). Hyper-threads share a
core's execution units, so on a 6-core, 12-thread host the count is 6:
3 automatic slots, each 2 jobs wide.

`greengate.hostCores N` (1 to 1024) replaces the detected count, and
`GREENGATE_HOST_CORES` overrides it for one process. The slots crate
resolves both host keys for every client
(`greenharness_slots::host::resolve`), so greengate and relais always size
the queue from the same numbers. Detection reports how it found the count:
`detected`, `detected (logical fallback)` where the OS gave no physical
count, or `limited by affinity` where an affinity or cgroup limit allows
fewer cores.

`greengate slots` names the threads only where they differ from the cores,
and tags a count that plain detection did not set, inside 80 columns
(`6 cores, 12 threads`, `4 cores, 12 threads (env)`, `(git)`, `(logical)`,
`(affinity)`). `--json` carries `cores` and `cores_source`, whose value is
a stable token: `env`, `global`, `system`, `detected`, `logical_fallback` or
`limited_by_affinity`. `greengate doctor` prints the full sources on
stderr, one line per key:

```text
host cores: 4, 12 threads (GREENGATE_HOST_CORES)
host slots: auto (detected)
```

An invalid value is a warning that names the fix for its source
(`unset GREENGATE_HOST_CORES`, or the `git config` command).

## Who holds a slot

Only the process that took the slot holds it. Slot descriptors are
close-on-exec, so a command's children, and any daemon it leaves behind,
never hold it. `greengate slot run` frees the slot when its command exits.
A holder exports `GREENHARNESS_HOST_SLOT=held:<lease>` to the command it runs,
naming its lease id (`<pid>-<seq>`, the `lease` of its `holders/` records). A
greengate, relais or wrapper started under it does not queue again, once the
crate's one reader (`held_marker`) finds that lease live: a holder record
naming it, its writer alive, and one of its slots locked. Any other value
(`1`, a path, a dead lease, typed by hand) queues like no marker, with two
lines on stderr and one `marker_refused` row in `waits.jsonl` naming the
reason (`malformed`, `unknown_lease`, `dead_pid`, `slot_free`, `io`, `bare`):

```text
greengate: GREENHARNESS_HOST_SLOT=1 names no live lease (malformed); queueing
  unset it; only a slot holder sets it
```

The bare `held` older binaries export is accepted until both tools write
the new form (greengate 5.7, relais's t15 phase-2 release), then refused
(`bare`). Meanwhile an accepted bare `held` leaves one `marker_bare` row
(no `reason`) per process, so the holders still writing it can be counted.
Tests escape the real queue with slots off (`GREENGATE_HOST_SLOTS=0`), never
with a marker.

## Handoff inside a rehearsal

Inside a rehearsal snapshot a pre-push keeps its slot from one heavy gate to
the next. The finished gate parks its lease and renames the holder record to
`pre-push, between checks`; the next heavy gate takes it with a `handed` line
in `waits.jsonl` (a wait of 0, the same `pid`). The pre-push releases a parked
slot when it ends, however it ends. While a slot is parked, a light check's
tools run under it, width-bound, and a child `greengate slot run` does not queue. A gate that runs on the cluster executor releases the parked slot first.
A plain push and a commit release their slot after each check.

## When a wait ends

A hook check's wait ends, from 5.1.1, at its ceiling and nowhere before: it has no
stall window (ADR-0022, amended). `greengate slot run` and relais keep the
stall rule. A line has **stalled** when no
admission anywhere (the `admitted-at` stamp) and no change of the waiter's
place for 300 s. The waiter's own marks run on a monotonic clock, so a
laptop's sleep cannot stall a moving line; a stamp ahead of now counts as just
now. `GREENHARNESS_SLOT_STALL_S` overrides the window. It is a test seam beside
`GREENHARNESS_SLOT_DIR`, read by each binary at its edge. A wait also ends at
its caller's hard **ceiling**, or when three polls in a row fail after the
queue was joined. One loop in the crate, `wait_in_line`, applies the rule for
every caller; `wait_in_line_paced` is the same loop with the pause between
polls left to the caller, which relais uses to heartbeat its lease, charge its
budgets and honour a cancel while it waits. Whatever ends a wait, nothing runs outside the queue because of
it:

| Caller | Stall window | Ceiling | At the end |
|---|---|---|---|
| hook check, `onBusy wait` (and every rehearsal) | none | `greengate.timeout` (3600 s when 0) | defers to CI |
| heavy `warn` check of land's pre-stage | none | `greengate.warnSlotWait` (600 s, never above the timeout) | defers |
| hook check, `onBusy defer` | none | `slotWait` (60 s) | defers |
| `greengate slot run` | 300 s, never above `--max-wait` | `--max-wait` (90 s) | exits 75 (74 when the queue failed) |
| relais verification command | 300 s | the run's total budget | blocks `admission_wait_exhausted` |

A gate that defers inside a rehearsal or land's pre-stage (only there) writes a
`deferred` completion record for the tree (ADR-0018's `refs/notes/amont-gate-done`),
which answers for an hour: land's push does not refuse the gate it could not
run, and CI runs it. It is never a stamp or an attestation.

Skipping the queue is a choice made up front, journalled `now`:
`GREENGATE_ON_BUSY=now` for every heavy hook check of one run (never in git
config or `greengate.conf`, so no committed file skips the queue), and
`greengate slot run --now`.

`greengate.warnSlotWait`: git config, then `set warnSlotWait <s>` in a trusted
`greengate.conf`, then 600.

## Width

A slot bounds how many heavy jobs run, and the width bounds how hard each
one runs. Left alone, `cargo` builds and its test harness runs on every
core, so `cap` holders ran `cap × cores` jobs at once. A holder of `weight`
slots therefore runs `jobs = max(1, weight × cores / cap)` wide, with `cores`
as [Cores](#cores) counts them. greengate sets
`MAKEFLAGS=-j{jobs}`, `CARGO_BUILD_JOBS`, `RUST_TEST_THREADS`,
`NEXTEST_TEST_THREADS`, `VITEST_MAX_WORKERS` and `GOMAXPROCS` on every heavy
hook check and every `greengate slot run` command
(`amont_slots::bound_width`). relais uses the same formula for its
verification commands.

vitest reads `VITEST_MAX_WORKERS` after its config, so the slot's width
outranks a project's `maxWorkers` and also its `fileParallelism: false`. A
suite that must run its files one at a time sets `VITEST_MAX_WORKERS=1` in its
own `test` script, which greengate keeps (see Precedence). `go build` and
`go test` default `-p` to `GOMAXPROCS`, and each test binary inherits it, so it
caps `-parallel` as well.

- **Precedence:** the check's own environment first, then the parent's
  environment, then greengate's computed width. A key already set to a
  non-empty value is kept, so a check that sets its own width, or a relais
  run that bounded its command, wins. An empty value counts as unset.
  A `MAKEFLAGS` that carries flags but no job bound gains ` -j{jobs}`.
- **Unqueued runs are bounded too.** A check that runs after its slot wait
  timed out runs when the host is busiest.
- **Nothing is set** when slots are off (`greengate.hostSlots 0`), unsupported
  (Windows), or the command is nested under a holder (a live
  `GREENHARNESS_HOST_SLOT=held:<lease>`), whose bounds it inherits.
- `greengate slots` shows each holder's `jobs`, or `-` for a record written
  without it.

## Old clients

greengate up to 1.61 and relais before it linked this crate take `slot-0..N`
with no ticket and no load test. They jump the queue until they are
upgraded. Their slots show as `unknown (old client)`, because they write no
holder record.

## Commands

- `greengate slot run [--weight N] [--max-wait S | --now] -- <cmd…>` waits for a
  slot, then runs the command with the held marker set and passes its
  exit status through. When the line stalls for 300 s, or `--max-wait` runs out
  (default 90 s, under Claude Code's 120 s Bash timeout), it exits 75 and runs
  nothing, saying which. `--now` runs it at once, unqueued and width-bound. From
  5.0 `--on-timeout run` exits 2, and `--on-timeout exit` warns that it is
  deprecated.
- `greengate slot promote <id> --reason <text> [--to N]` moves one of the
  session's own waiting jobs ahead; see [Moving a job ahead](#moving-a-job-ahead).
- `greengate slots [--json]` lists the holders, the old-client slots, the
  number of waiters and, from 5.2, the queue: one line per job, `position`,
  `id`, wait, session and command, within 80 columns (`gone` for a waiter
  that died, unnumbered; `old client` for an empty ticket). `--json` gains
  `queue`: `id`, `position` and `waited_ms` (both `null` when gone), `session`,
  `tool`, `command`, `repo` (`null` for an old client's ticket) and `gone`.
  The listing takes no lock and changes nothing.
