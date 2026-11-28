---
status: active
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
`out`, every batch's, which `refine` reads for the skip tables, and `full`,
only the batches with findings, plus a `gap` reason); rust, go and js keep
calling the wrapper. A `TimedOut` directory is named NOT checked and the
loop goes on, so other directories' findings survive (today
`audited_in(...)?` drops them). A directory with a gap keeps its findings
(they still block a `v*` tag); the merged result is then never waivable, so
a waiver covering batch 0's ids cannot pass a release whose batch 1 was
never audited.

**What the person sees.** A marked file reads like the unmarked one: the
finding lines and table and the skipped-package line, then
`audit-python: <dir>/requirements.txt: audited (N package versions)`, as the
lock route prints. A gap says why with the lock route's lines (out of time +
`TIMEOUT_NEXT`; no private directory or batch file + `TMPDIR_NEXT`; pip-audit
could not answer + its clipped reason + `pip_audit_next`), and the
directory is then named "NOT fully checked".

## Phases

- [ ] Phase 1 — marked hashed files audited in batches:
  `crates/greengate-runtime/src/hooks/audit.rs` (unit tests there),
  `crates/greengate/tests/it/audit.rs` (end-to-end test), `CHANGELOG.md`

## Decision log

- 2026-11-28 — Keep the unmarked hashed route as f108 built it, because it
  has no duplicate names to batch and its behaviour is already verified; the
  batch route is taken only when a marker is present.
- 2026-11-28 — Multiple `-r` flags in one pip-audit run were probed and
  refused (`package jinja2 has duplicate requirements`), so batches are
  separate runs.

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

## Outcome

<!-- panel: repos=amont reviewers=backend,lang:rust,tui,unix body-sha=c800ac9aa2e7 -->
