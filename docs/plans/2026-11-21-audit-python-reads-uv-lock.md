---
status: active
branch: feat/audit-python-uv-lock
repos: [amont]
adrs: []
plan-id: p-01M7XE82JR1MSNEV9TX9KYYSTX
---
# audit-python reads uv.lock when there is no virtualenv

## Review panel
👉 **Decide:** approve the growth, 962 → 1,533 words (×1.59). Every addition answers a reviewer finding (5 high), or ask for a smaller plan.
📍 amont · 2 phases · 1 release · next: Phase 1. Panel: architect, backend, rust, product owner, tui, unix (opus), codex.
**Changed by review:** PyPI-only names sent (ADR-0055), one audit per `uv.lock` directory, and an empty export is Unavailable, never Clean. Markers are stripped and every member is exported (`--all-packages`).
📄 Full reviews: [2026-11-21-audit-python-reads-uv-lock.reviews.md](2026-11-21-audit-python-reads-uv-lock.reviews.md)
**Verdicts:** round 1: 6 approve-with-changes; round 2: 6 approve-with-changes; codex approve-with-changes; backend approve on the final body. 13 carried; remove 0, merge 0.

## Picture

```
python_run
  requirements.txt? --> pip-audit -r requirements.txt   (unchanged)
  venv?             --> pip-audit --path <site-packages> (unchanged)
  uv.lock + uv?     --> uv export, all groups -> tmp     (NEW)
                        keep PyPI names, count the rest
                        pip-audit -r tmp --no-deps --disable-pip
                        release: dev-only findings filtered
  none of these     --> warn "did NOT run", Unavailable
```

The gate tries each source in order and stops at the first it finds; only
the third row is new, and it builds no virtualenv.

## Context

`pre-push-audit-python` audits a uv project through its installed `.venv`
(`crates/greengate-runtime/src/hooks/audit.rs:1247`). A fresh worktree has no
`.venv`, so the gate warns "no requirements.txt, and no virtualenv to audit —
the audit did NOT run" and the push goes through unaudited. On 2026-11-20 this
happened on sre-agent PR #107, pushed from a new worktree.

Measured by hand on sre-agent's `uv.lock`, with no venv:

```
uv export --frozen --no-emit-workspace --no-dev --format requirements-txt -o req.txt
pip-audit -r req.txt --no-deps --disable-pip
```

- 80 packages audited, nothing resolved, no venv built.
- The private `platform-kernel` was skipped with "Dependency not found on PyPI",
  not a crash.
- It found 10 known vulnerabilities in 5 shipped packages, which the gate has
  not been seeing.

`--no-deps --disable-pip` avoids the failure the comment at `audit.rs:1248`
describes: pip-audit no longer resolves the exported file in a throwaway venv,
so workspace members and private-index packages can't make it die on "No
matching distribution found".

## Phases

### Phase 1: the lock route

Files: `crates/greengate-runtime/src/hooks/audit.rs`.

In `python_run`:

- When `venv_site_packages` returns None, `has_lockfile("uv.lock")` holds and
  `common::which("uv")` finds uv, audit each directory of
  `lockfile_dirs("uv.lock")` on its own, through `audited_in` with that
  directory as cwd and under the existing time budget (a timeout is
  `Ran::Unavailable`):
  1. Make a private directory with `fs::create_dir(temp_dir().join(
     unique_name("greengate-audit-python")))`: the helper at
     `pushed_tree.rs:315`, moved to `common`. It fails if the path exists, so
     nothing can be planted there. A `Drop` guard removes it on every return
     and panic. A signal leaves it behind, as `capture.rs:18` notes.
  2. Run `uv export --frozen --all-packages --all-groups --all-extras
     --no-hashes --no-emit-workspace --format requirements-txt
     -o <dir>/export.txt`. Add `--all-packages` to the production-name
     export too, so audit scope and release classification cover the same
     workspace members. Phase 2 includes a shared workspace lock with a
     vulnerable dependency used only by a member that the root does not
     depend on; verify with real uv that both exports include it, and that
     the release gate blocks its vulnerability. Dev
     groups are included, as in a venv. If the export fails, print one
     warning line (`failure_line`, clipped to 100 characters) ending "re-run
     `uv export --frozen` to see why", and return `Ran::Unavailable`.
  3. Keep PyPI packages only. ADR-0055 says workspace, path, git and
     private-registry packages are never sent, and pip-audit looks each name
     up on PyPI. Without this step it would send sre-agent's `platform-kernel`,
     which `uv.lock` records as `source = { registry = "https://git.daddyshome.fr/…" }`.
     - A std-only line scan of that directory's `uv.lock` `[[package]]`
       blocks (no TOML crate: `scripts/check-no-deps.sh`) gives the names
       whose blocks all have `registry = "https://pypi.org/simple"`, with or
       without a trailing slash.
     - Reuse the `uv_production_names` name extraction and `normalise`
       logic (`audit.rs:1405`), then validate exact `name==version` pins.
       Keep only names admitted by the lock-source filter. Remove environment
       markers from admitted pins so all locked platform and Python-version
       variants are audited, independently of the audit host. Deduplicate
       identical (name, version) pairs; unsupported rows remain unclassified.
       Partition different versions of the same name into separate audit
       files, with at most one version per name in each file: pip-audit
       rejects conflicting pins in one requirements input. Delete the raw
       export once filtering completes.
       Phase 2 includes platform and Python-version forks, including two
       versions of one name, and verifies every admitted pair reaches the
       auditor exactly once. Verify these fixtures with real pip-audit as
       well as the command shims.
  4. Print two separate warnings: "N private/non-PyPI packages skipped (a, b,
     c +K more)", and "N packages could not be classified from uv.lock; not
     audited". An unknown is never sent. If no package is kept, warn "did NOT
     run" and return `Ran::Unavailable`: an empty file is never Clean.
  5. For each batch, run `pip-audit -r <batch-file> --no-deps --disable-pip`
     through `audited_in` under the existing time budget and read it with
     `read_pip_audit`. Merge batch findings without losing known
     vulnerabilities when another batch fails; incomplete coverage must
     never become Clean. One line names the source: "audit-python: audited
     uv.lock (80 package versions, 1 skipped)". Count successfully checked
     distinct (name, version) pairs, not retained requirement lines. It does not rely on colour, and quiet
     mode hides it. Warning text shows `uv.lock`, never the temp path.
- Release filtering: the existing `venv_mode` branch (`audit.rs:1279`) applies
  to the lock route too. On a tag, findings outside `uv_production_names` are
  warned as dev-only and do not block, the same as the venv route. Set the
  flag from the branch taken (venv or lock), not from `argv` containing
  `--path` (the lock route's argv has `-r`, like requirements.txt), and name it
  `installed_or_locked`. When `uv_production_names` returns None, every
  finding is kept: an unknown is not a pass.
- The warning when nothing is available names all three sources it looked
  for (requirements.txt, a virtualenv, uv.lock with uv on PATH) and ends with
  the next command: `uv sync`, or install uv so uv.lock can be audited. Each
  line is 80 columns or fewer.
- Order is unchanged: requirements.txt first, then the venv (the installed
  versions are the truer answer), then the lock.

### Phase 2: tests (same PR and commit as Phase 1)

Files: `crates/greengate/tests/it/audit.rs`, `crates/greengate-runtime/src/hooks/audit.rs`.

- Integration test in `crates/greengate/tests/it/audit.rs`, beside
  `a_uv_project_audits_its_venv_not_a_missing_requirements_file` (line 289):
  a committed `pyproject.toml` and `uv.lock`, no `.venv`, with shims for `uv`
  and `pip-audit`.
  - The `uv` shim writes a requirements file to its `-o` argument, and prints
    the production list to stdout when called without `-o` (the release
    split's call).
  - The `pip-audit` shim answers only when `--no-deps`, `--disable-pip` and
    `-r <file>` are all present, in any order, so the test proves the lock
    route was taken without pinning argv order.
  - A vulnerable answer refuses a `v` tag.
- A release case: the export of production names leaves out the flagged
  package, so the tag push passes and the output says "dev-only".
- Private packages are never sent. A `uv.lock` holding one PyPI package and
  one private-registry package, plus a `pip-audit` shim that records its `-r`
  file. The recorded file names the PyPI package only.
- Unit test of the lock scan: registry, git, path, editable and virtual
  sources, a name with two blocks, a block with no name. Only
  `https://pypi.org/simple` passes, and a name counts only if all its blocks do.
- Mirror case: every package non-PyPI gives "did NOT run", not Clean.
- An export failure: the `uv` shim exits 2, and the gate warns "did NOT run"
  and does not block.
- Update the existing "no venv" case at line 310 to also have no `uv.lock`, so
  it still covers the nothing-available warning.
- Unit test: the temp directory is gone after the pass, vulnerable and
  export-failure paths, and an existing path at the chosen name is refused,
  not written through.

## Non-goals

- sre-agent's 10 findings (certifi, opentelemetry-api, pyjwt, setuptools,
  urllib3). A ticket in the sre-agent cve-lane map (a fix or a dated waiver)
  exists, with a due date, before greengate is installed.
- The venv route's exposure: `pip-audit --path` looks up every installed
  non-editable package, private ones included, which contradicts ADR-0055
  today. It is filed in amont, with a due date, when this plan merges.
- The broken local pip-audit install (its uv tool interpreter was gone). It was
  reinstalled on 2026-11-20 and is not a code change.

## Code review
- amont: backend, lang:rust, unix, tui (architect left out: no new module or contract, one function grows a branch)

## Release

greengate minor release through the `tag-release` skill, then install it on this
machine.

Effect: a uv repo pushed without a `.venv` now has its tag pushes audited.
sre-agent's next `v` tag is refused until its 5 shipped packages are fixed or
waived. Before install, the sre-agent cve-lane follow-up ticket must exist with
a due date.

## Verification

- `cargo test -p greengate-runtime` and `cargo test -p greengate --test it audit`.
- Lock with only private packages: expect "did NOT run" warning, pip-audit shim never called.
- Lock with a PyPI mirror source: expect all names skipped, a count in the warning, no block.
- Real sre-agent `--all-groups` export: record the finding count and compare with the hand run's 10.
- Live: in a fresh sre-agent worktree with no `.venv`, run `greengate land` on a
  docs-only branch, using the released greengate. The audit must run on the
  lock, report at least the 10 known findings in certifi, opentelemetry-api,
  pyjwt, setuptools and urllib3 as a warning on a branch push, print "1
  private/non-PyPI package skipped (platform-kernel)", and not print
  "did NOT run". (The wording was corrected at implementation. Round 2 of
  the panel carried the change.) The run with the released binary, through
  `greengate land`, is part of the release check in `tag-release`.

### Results (2026-11-21, commit 1ed919f5)

Input → expected → actual:
- Full suite, via relais run run-660c6948e4bff-6501 → the default profile
  passes → `cargo-fmt`, `clippy`, `cargo-test`, `check-no-deps.sh` and
  `attest-inputs` all exited 0, and the pre-commit gate passed 29 checks.
- Lock with only private packages → "did NOT run", no block →
  `an_all_private_lock_did_not_run_and_never_blocks` passes.
- Mirror lock → known gap, "did NOT run", no block → the same test. A mirror
  URL is not `pypi.org/simple`, so it is skipped like a private one.
- Lock route, vulnerable tag, dev-only tag, export failure and nested lock →
  `a_uv_lock_without_a_venv_is_audited_and_a_vulnerable_one_refuses_a_tag`,
  `a_dev_only_finding_in_the_lock_passes_a_tag`,
  `a_failed_export_warns_and_does_not_block` and
  `a_nested_uv_lock_is_audited_from_its_own_directory` all pass.
- Unit tests → the lock scan, marker batching, the skipped line and the 0700
  private directory → all four pass.
- Live, piloted → the branch build, not the release, on a fresh sre-agent
  worktree with no `.venv`, through `greengate run pre-push-audit-python`:
  - It printed "1 private/non-PyPI package skipped (platform-kernel)" and
    "Found 10 known vulnerabilities in 5 packages — this will BLOCK a v* tag
    push". `--all-groups` found the same 10 as the hand run.
  - No "did NOT run". Exit 0 on a branch.
  - No `greengate-audit-python` directory was left under `$TMPDIR`.
  - The "audited uv.lock" line did not show, because quiet mode was on when
    the output was not a terminal. That is the plan's quiet rule.
- Real uv, workspace member → a scratch workspace whose member `m` depends on
  `urllib3==2.7.0`, and whose root does not depend on `m`, so `--all-packages`
  is needed → without the flag the export has `idna` only; with it, it has
  `idna` and `urllib3==2.7.0`.
- Real pip-audit, forked versions → batches audit apart, and one file with
  both versions fails.
  - `batch-1` (`urllib3==2.7.0`, `idna`): "Found 5 known vulnerabilities in 2
    packages".
  - `batch-2` (`urllib3==1.26.5`): "Found 18 known vulnerabilities".
  - One file with both versions: `ERROR … duplicate requirements`.
- After the review fixes (commit a8c15c23), the relais run
  run-660c7332cced0-149a4 passed every check, including a new two-batch test
  where one batch is vulnerable and the other fails. The live pilot ran again
  on a fresh sre-agent worktree with the same result: platform-kernel
  skipped, 10 findings, exit 0, and no leftover directory.
- Export failure clipping: the plan's "100 characters" became a reason line
  clipped so the line fits 80 columns, as carried from round 2 and asked
  again by implementation review.

## Measure

- 7 days after install: count audit-python "did NOT run" warnings in uv repos
  against the 7 days before. Expect near zero.
- On each lock-route run, record packages audited and skipped. Roll back, or
  fix forward, if a repo audits 0 packages or skips more than its known
  private packages.

<!-- panel: repos=amont reviewers=architect,backend,lang:rust,po,tui,unix,codex body-sha=81b66168eff7 -->
