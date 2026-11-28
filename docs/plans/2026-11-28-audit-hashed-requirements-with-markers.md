---
status: done
branch: fix/audit-hashed-markers
repos: [amont]
adrs: [ADR-0055]
plan-id: p-01M8GVFEJ25GK5QMVBV1VBRKPH
---
# Audit hashed requirements with markers

## Review panel
👉 **Decide:** none — approve if batching a marked hashed file's stripped pins into separate pip-audit runs is the right fix.
📍 amont · 1 phase · next: Phase 1. Panel: backend, lang:rust, tui, unix (opus), codex.
**Changed by review:** gap lines reuse the lock route's reasons and Next lines; a timed-out directory no longer drops others' findings; answer carries `out` and `full` apart.
**Verdicts:** round 2: 1 approve, 3 approve-with-changes; codex approve-with-changes (high on name provenance rejected: route unchanged, follow-up); 8 carried into implementation; words 953 → 1110 (×1.16); remove 0, merge 0.
📄 Full reviews: [2026-11-28-audit-hashed-requirements-with-markers.reviews.md](2026-11-28-audit-hashed-requirements-with-markers.reviews.md)

## Picture

```
requirements.txt (per directory)
  not fully hashed / option line ........ resolve, as today
  hashed, no marker ..................... pip-audit -r requirements.txt
                                          --no-deps --disable-pip (f108)
  hashed, with markers (new) ............ pins, markers stripped
                                          -> batches (one version per name)
                                          -> pip-audit -r batch-N.txt
                                             --no-deps --disable-pip, each
                                          -> reports merged; a gap keeps
                                             findings, never waivable
```

The picture shows the three routes a requirements.txt takes; only the
third is new.

## Goal

A fully hashed `requirements.txt` whose pins carry environment markers (what
`uv pip compile --universal` writes) is audited without a resolve, with every
platform's pin included. Today such a file falls back to the resolve route:
on the host, pip-audit either fails on a pin with no host wheel or, under
`--no-deps`, drops an off-host pin without saying so
(`jinja2==2.10 ; sys_platform == 'linux'` read "No known vulnerabilities
found" on the Mac). Decisions follow-up f118.

## Non-goals

- The unmarked hashed file keeps its f108 route (`pip-audit -r requirements.txt
  --no-deps --disable-pip` in its own directory): it cannot hold two pins of
  one name, so it needs no batches, and its test stays as it is.
- Unhashed files, option lines (`-r`, `-c`, `-e`, index URLs), URL or path
  requirements and non-exact pins still resolve, as today.
- The uv.lock route, rust, go and js audits do not change behaviour.
- No marker is evaluated: every pin is audited whatever host it is for.

## Code review

- amont: backend, lang:rust, tui, unix

## Behaviour

**Which files take the new route.** `hashed_pins_in` stops refusing a pin with
a marker. It returns the distinct `(name, version)` pairs (through
`exact_pin`, which already strips the marker and any `[extra]`), the number
of distinct names, and whether any pin had a marker; it still answers None
for an option line, an unhashed line, a non-exact pin and an empty file. The
file is read once per directory.

**The marked route, per directory.** For a hashed file with any marker:

- the pairs are split into batches by a `batches_of(pairs)` helper extracted
  from the end of `filter_export` (one version of a name per batch: pip-audit
  refuses `package jinja2 has duplicate requirements`, also across several
  `-r` flags of one run, probed with pip-audit 2.10.1). `filter_export`
  itself cannot read a hashed file: it reads physical lines, so
  `six==1.16.0 \` fails `exact_pin`, and it needs the lock's PyPI set;
- each batch is written, without markers or hashes, as `batch-N.txt` in a
  `PrivateDir` that lives for the directory's whole audit, and audited with
  `pip-audit -r <batch> --no-deps --disable-pip` from the requirements
  directory;
- every output is scrubbed of the private path (`scrub_private`);
- each batch is read on its own (`read_pip_audit`) and the reports are
  merged; outputs are never concatenated before reading, since a clean
  batch's "No known vulnerabilities found" would mask a finding in the next;
- one `Budget` (the `greengate.timeout` ceiling) covers one marked
  directory's batches, as the lock route has; other directories keep their
  per-call timeout. It is checked before each batch and each run keeps its
  own timeout, so the wall time stays under about twice the ceiling. A
  batch that times out or a spent budget stops the directory, keeps what
  earlier batches found, and marks it incomplete; `TimedOut` is returned
  only when no batch answered;
- the skip tables of all batches feed `pip_skipped_names`, as today;
  "every pin skipped" compares against the distinct names, not lines.

**Merging.** `audit_each_dir` becomes a thin wrapper over a core that takes a
per-directory answer (`Nothing`, `TimedOut`, or a report with two outputs:
`out`, every batch's, read for the skip tables inside the answer, and `full`,
only the batches with findings, plus a `gap` reason); rust, go and js keep
calling the wrapper. A `TimedOut` directory is named NOT checked and the
loop goes on, so other directories' findings survive (today
`audited_in(...)?` drops them). A directory with a gap keeps its findings
(they still block a `v*` tag); the merged result is then never waivable, so
a waiver covering batch 0's ids cannot pass a release whose batch 1 was
never audited.

**What the person sees.** A marked file reads like the unmarked one: the
finding lines and table and the skipped-package line, then
`audit-python: audited <dir>/requirements.txt (N package versions, K skipped)`,
as the lock route prints. A gap says why with the lock route's lines (out of time +
`TIMEOUT_NEXT`; no private directory or batch file + `TMPDIR_NEXT`; pip-audit
could not answer + its clipped reason + `pip_audit_next`), and the
directory is then named "NOT fully checked".

## Phases

- [x] Phase 1 — marked hashed files audited in batches:
  `crates/greengate-runtime/src/hooks/audit.rs` (unit tests there),
  `crates/greengate/tests/it/audit.rs` (end-to-end test), `CHANGELOG.md`

## Decision log

- 2026-11-28 — Keep the unmarked hashed route as f108 built it, because it
  has no duplicate names to batch and its behaviour is already verified; the
  batch route is taken only when a marker is present.
- 2026-11-28 — Multiple `-r` flags in one pip-audit run were probed and
  refused (`package jinja2 has duplicate requirements`), so batches are
  separate runs.
- 2026-11-28 — Non-goals amended by the work: two changes reach every
  per-directory audit (rust, go, python's requirements files) through the
  shared core: a directory whose run timed out is named NOT checked and the
  loop goes on, and a result with a gap (not `CouldNotCheck`) is never
  recorded, so the next push audits again instead of replaying it for a day
  (implementation-review and backend, round 1). JavaScript keeps its own
  loop. `push_gates::a_partly_answered_audit_is_not_recorded` drives it
  through audit-go.
- 2026-11-28 — The skip-table check moved into the python answer, so its
  `refine` is the identity and no state is shared between the closures
  (lang:rust, round 1). The ok line is the lock route's shape:
  `audit-python: audited <path> (N package versions, K skipped)`, both
  counted in versions.
- 2026-11-28 — `posix_signal.rs` declares `read`/`write` with `c_void`: the
  current toolchain's clippy (`suspicious-runtime-symbol-definitions`)
  refused the pre-commit gate on main's declaration; outside Phase 1,
  needed to commit at all.
- 2026-11-28 — Codex's provenance finding (an off-host pin of a private name
  is now sent) rejected for this plan: the requirements route has sent
  every name of the file since before f108; filed as a follow-up.

## Verification

- Unit: `hashed_pins_in` on a hashed file with `six==1.16.0`,
  `numpy==1.26.4 ; python_version == '3.9'` and `numpy==2.1.0 ;
  python_version == '3.12'` → three pairs, two names, marked (the old
  marked → None assertion is inverted); `batches_of` → two batches, numpy
  once in each; an unhashed marked line → None.
- End to end (`crates/greengate/tests/it/audit.rs`): a hashed file with
  `jinja2==2.10 ; sys_platform == 'linux'` and a name with two marker-split
  versions; a pip-audit shim that refuses a run without `--no-deps
  --disable-pip`, refuses duplicate names, prints "No known vulnerabilities
  found" for any input holding `;` (the drop), and prints the real jinja2 2.10
  rows (captured from pip-audit 2.10.1) for an input holding `jinja2==2.10` →
  a `v*` tag push is refused and shows PYSEC-2019-217. On current main the
  same test fails (the marked file takes the resolve route).
- End to end, the batch holding numpy's second version sleeps past
  `greengate.timeout` → refused, PYSEC-2019-217 shown, the directory "NOT
  fully checked"; with a dated waiver for jinja2's ids, still refused.
- End to end, two directories: A finds jinja2, B's run times out → refused
  naming PYSEC-2019-217, B named NOT checked.
- End to end, `TMPDIR` read-only on a marked file → not clean, stderr names
  the cause and `TMPDIR_NEXT`.
- Piloted on this Mac, built binary and real pip-audit 2.10.1, in a scratch
  repo: a hashed `requirements.txt` holding `jinja2==2.10 ; sys_platform ==
  'linux'`, a `v1.0.0` tag push → baseline (pip-audit alone) "No known
  vulnerabilities found"; expected: refused naming PYSEC-2019-217.
- Timing: a real `uv pip compile --universal` file → batch count and wall
  time against `greengate.timeout`, recorded in Outcome.

Results (2026-11-28, this Mac, host load 10-24):

- Unit → as expected: 30/30 `hooks::audit` pass, including
  `marked_pins_are_stripped_and_their_versions_batched_apart` (3 pairs,
  2 names, marked; 2 batches).
- End to end → as expected: `a_hashed_file_with_markers_audits_every_platforms_pin`
  (refused, PYSEC-2019-217, the shim recorded exactly `jinja2==2.10`,
  `numpy==1.26.4`, `numpy==2.1.0`; a second case whose only finding is in
  the last batch is refused with PYSEC-2099-1); on main the marked file
  takes the resolve route the shim refuses, so the tag is not refused (by
  construction, not run on main).
- Timeout → as expected: `a_marked_batch_that_times_out_keeps_the_findings_and_is_never_waived`
  (refused, "timed out; unaudited versions remain — NOT fully checked";
  waived: still refused; same waiver with every batch answering: passes).
- Two directories → as expected: `a_timed_out_directory_does_not_drop_another_directorys_findings`
  (refused, `sub (timed out)`, waiver still refused).
- `TMPDIR` 0500 → as expected: `a_marked_file_with_no_private_directory_says_why`.
- No record of a gap → `push_gates::a_partly_answered_audit_is_not_recorded`
  passes; `audit::` + `push_gates::` 74/74; clippy `-D warnings` clean;
  pre-commit gate 29 checks passed.
- Pilot → as expected: pip-audit alone on the scratch file printed "No known
  vulnerabilities found"; the built binary on a `v1.0.0` push refused it
  with `Found 12 known vulnerabilities in 1 package — not waived:
  PYSEC-2019-217, PYSEC-2021-66, …`, rc 1, 2.5 s.
- Timing → under the ceiling: `uv pip compile --universal --generate-hashes`
  of numpy (split by Python version), requests, fastapi and pywin32 (win32
  only): 34 pins, numpy in 4 versions, 4 batches, 10 s wall on a branch
  push (21 findings in 4 packages), against `greengate.timeout` 3600.

## Implementation review

- implementation-review (opus): round 1 approve-with-changes (4: plan record, Non-goals, waiver control fixed; ok line); delta approve-with-changes, plan items fixed here. 94k+49k, 66+73 s.
- backend (opus): round 1 approve-with-changes (high: a gap was recorded and replayed 24 h, fixed; units, gap reason fixed; 2 s timeout deliberate: flake margin); delta approve; low (name-matched skips with two versions) deliberate. 69k+35k.
- lang:rust (opus): round 1 approve-with-changes (Cell removed, guard, Budget doc fixed; clone, enum deliberate); delta approve; lows (one fact in two fields, Coverage enum) deliberate. 51k+32k.
- tui (opus): round 1 approve-with-changes (double Next, unclipped gap, units, wording fixed); delta approve; low (say "not recorded") deliberate. 64k+33k.
- unix (opus): round 1 approve; low (budget per marked directory) deliberate: the plan panel chose it. 62k.

## Outcome

Shipped on `fix/audit-hashed-markers`: a fully hashed requirements.txt with
environment markers is audited from its marker-stripped pins in batches,
every platform's pin included; a gap keeps findings, is never waivable and
is never recorded; a timed-out directory no longer drops the others'
findings. Surprise: main's `posix_signal.rs` fails clippy on the current
toolchain, so every Rust commit in amont was blocked at pre-commit-clippy
until it was fixed here. Not done: name provenance on the requirements
route (follow-up).

<!-- panel: repos=amont reviewers=backend,lang:rust,tui,unix body-sha=c800ac9aa2e7 -->
