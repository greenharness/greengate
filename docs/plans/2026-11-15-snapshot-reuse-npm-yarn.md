---
status: active
branch: feat/snapshot-clone-only
repos: [amont]
adrs: []
---
# `snapshotDeps reuse` for npm and yarn 1

## Review panel

📄 Full reviews: 2026-11-15-snapshot-reuse-npm-yarn.reviews.md

👉 **Decide:** resolved 2026-11-15: po's value gate is met for yarn, since happier (7.3 GB `node_modules`, `packageManager: yarn@1.22.22`, `greengate.snapshotDeps = reuse` in its `.git/config`) is a yarn 1 repo that installs in full on every push today; the npm repos are small root `package-lock.json` projects (fredericrous.com, activity-vision, agent-console, thomasgrougi.com, governance-ts). Approve if the bet is that Phase 0's measurements (disk, time, false accepts) gate the npm and yarn work, and the `clone_dir` full-copy removal ships either way. JSON from npm/yarn is read by one `node -e` helper through a stdout-only capture (architect's one-key Rust scan was rejected).
📍 amont · plan drafted, nothing built · next: Phase 0 measurement (disk and time, in a scratch dir), then Phase 1a, the `clone_dir` disk fix. Panel: backend, lang:rust, unix, tui, po, architect (opus-5-5; round 3 and the last backend binding on fable-5-1).
**Changed by review:** a go bar of zero false accepts plus 50% time or 10% disk; yarn's `check` proven against in-range drift; Phase 1 split so the disk fix ships alone; CI reflink mount for the tests. **Changed at your request:** the goal is disk, so `clone_dir` loses its full-copy fallback for every manager, pnpm included.
**After approval:** Phase 0 measured (macOS `cp -c` silently copies across volumes; `clonefile(2)` refuses it, 16x faster, +2% disk); Phase 1a is now `clonefile(2)`; backend, rust and unix re-reviewed that delta, 0 blockers.
**Verdicts:** round 1 approve-with-changes 6; round 2 approve-with-changes 6; round 3 approve-with-changes 6, 0 open blockers (one blocking and one high applied); 1 decision for you (po's value gate); 8 medium/low carried into implementation (in the full reviews). codex: not installed.

## Context

`greengate.snapshotDeps reuse` clones the working tree's `node_modules` into
the push snapshot instead of running a fresh install, but only for pnpm. npm
and yarn always install, so a push gate in an npm or yarn repo pays a full
`npm ci` / `yarn install` per snapshot.

**The goal is disk first, time second.** The data volume is 99% full (7.3 GB
free, 2026-11-15), and the person asked for the snapshot to reuse the
worktree's `node_modules` to regain space. On APFS `cp -c` is copy-on-write,
so a clone shares blocks with the worktree and should cost about nothing,
while `npm ci` and `yarn install` write a full private copy per snapshot.
pnpm is different and must be measured, not assumed: its fresh install
hardlinks from the store (`package-import-method=hardlink`, a 5.2 GB store
here), so it is already small, and reuse for pnpm is a speed and
peak-space-while-full question. The plan covers pnpm too: disk is measured and
reported for every manager, and pnpm's plain-copy fallback is removed (see
Disk).

Measured 2026-11-15, and it bounds what this plan can recover: a push
snapshot lives only for the push (`pushed_tree.rs:342`), so reuse lowers the
PEAK during a push and gives back nothing that is already gone. The standing
cost is the worktrees' own `node_modules`: 20 of them under
`~/Developer/Perso` add up to 23.3 GB by `du` (happier 7.3 GB,
happier-wt-m3-s2 6.3 GB, website-builder 1.8 GB; `du` counts a hardlinked
file once and cannot see clone sharing, so read it as an upper bound). And
`greengate.snapshotDeps` read unset in the 7 repos checked and globally at
first, but a later check found `reuse` in happier's `.git/config` (set since,
or misread: unknown). happier is yarn 1, where `reuse` still installs in full,
so its 7.3 GB tree is rebuilt per push. The person can switch `reuse` on now
for pnpm with `git config --global greengate.snapshotDeps reuse` (whether a
global value is honoured is a Phase 0 check); this plan makes that choice
safe on disk and extends it to npm and yarn. It recovers 0 GB of standing
space: pruning the two largest worktrees (happier 7.3 GB, happier-wt-m3-s2
6.3 GB) would recover about 13.6 GB, which is the person's call and outside
this plan.

Reuse is opt-in because a stamp must rest on the COMMITTED lockfile
(`snapshot_prep.rs` header). A clone is kept only if something vouches that
it is the tree the lockfile describes. For pnpm that is a frozen offline
install plus a layout check plus an in-place-edit scan. The source says no
such voucher exists for npm or yarn:

- npm: `npm ls` fails on peer-range conflicts that `npm ci` accepts
  (application-landscape: 182 findings), so it cannot accept a good clone.
- yarn: "`yarn check` is gone from both flavours".

**Finding that changes the plan:** the yarn claim is wrong for yarn 1.
`yarn check --help` on yarn 1.22.22 (this machine) lists `--integrity` and
`--verify-tree`. `--integrity` compares `node_modules/.yarn-integrity`
(lockfile hash, top-level patterns) with the lockfile. Only berry lacks it.
And for npm the right voucher is not `npm ls` but a dry-run install (below),
which this plan treats as a hypothesis to measure, not a fact.

## Picture

```
 step of reuse(unit)            who runs it          vouch, per manager
 -----------------------------  -------------------  ------------------
 lockfile tree == commit        shared shell         -
 install record present         per manager          see below
 no file newer than record      shared, param'd      -
 clone + link check + rollback  shared shell         -
 layout check                   pnpm only            -
 links outside the snapshot     npm, yarn1 only      -
 vouch for the clone            per manager          pnpm: frozen
                                                       offline install
                                                     npm:  install
                                                       --dry-run
                                                     yarn1: check
                                                       --integrity
 berry, bun                     install, unchanged   -
```

Every step is shared except the record file and the vouch, which each
manager supplies; any refusal removes every clone, says why, and runs the
normal install.

## Disk

What the code does today, read from `snapshot_prep.rs`:

- `clone_dir` (line 588) tries `cp -cR` and then falls back to `cp -R`, a
  FULL copy, without saying so; measured, `cp -c` itself also falls back to a
  full copy across volumes and still exits 0 (Phase 0 results). For pnpm that
  copy is larger than pnpm's own hardlinked install, and for any manager it
  silently defeats the point. Change: on macOS, `clone_dir` calls
  `clonefile(2)` on the directory with `CLONE_NOFOLLOW` (declared with a
  plain `extern "C"` block, not `unsafe extern`, since MSRV is 1.74 and the
  edition 2021; `#[cfg(target_os = "macos")]`; paths built with
  `CString::new(path.as_os_str().as_bytes())`, never `to_str()`, an interior
  NUL being a refusal; errno read with `io::Error::last_os_error()` at once;
  as `posix_signal.rs` and `load.rs` already declare libc functions; no new
  crate) and treats any error as a refusal: `EXDEV` (another volume),
  `ENOTSUP` (a filesystem without clones), `EEXIST` and `EACCES` leave
  nothing behind. After any other failure `clone_dir` removes `dst` only when
  the error is not `EEXIST` and `dst` did not exist before the call, so a
  failure can never delete a path greengate did not create.
  On Linux it runs `cp -R --reflink=always` (per-file, and only CI uses it
  today). On every other platform it refuses, as it does now. A refusal
  prints `no copy-on-write clone of <src> into <dst> (<os error>); installing
  instead` and the unit installs, so a gate run never writes a hidden full
  copy.
- The snapshot is removed when the push finishes (`pushed_tree.rs:342`); a
  crash or kill leaves it behind. Only one `greengate-push*` leftover exists in
  the temp dir today, so snapshots are not the leak.
- The 3,714 `greengate-*` entries in the temp dir are greengate's own test
  runs (migrate 1,400, slots 1,298, test 429, capture 294, sandbox 208,
  lists 82), not push snapshots. A sample of 200 totalled
  9 MB, so all of them are on the order of 170 MB: not the leak. Cleaning
  them is outside this plan.
- The standing cost is worktree `node_modules` (Context). Phase 0 sizes them
  by allocated bytes and reports the largest, so the person can decide what to
  prune; the plan deletes nothing.

What Phase 0 measures for disk, per manager and per mode (fresh install,
clone), by volume free space before and after (`df -k` on the data volume, a
quiet machine, repeated three times, against a no-op run's `df` delta as the noise floor): extra bytes the snapshot adds, and the
peak while the install or clone runs. The reuse line reports the result in
words only (`copy-on-write clone, shares disk with the working tree`), never a number, since measuring it in a
hook would cost a `du` per push.

## Phase 0 results (2026-11-15, this Mac, npm 11.9.0, yarn 1.22.22)

- **(g) `cp -c` across volumes: exit 0 and a silent full copy** (a 16 MB file
  landed as 16 MB on the other volume). Its exit code cannot refuse a
  non-clone. `clonefile(2)` called directly returns `EXDEV` (errno 18) across
  volumes, `EEXIST` onto an existing path, and creates nothing on failure.
  So on macOS the clone is `clonefile(2)` on the whole `node_modules`
  directory, not `cp -c`; the `st_dev` pre-check stays as a cheap early
  refusal, but `EXDEV` is now the authority.
- **Clone speed decides whether reuse pays at all.** `cp -cR` clones per file:
  74,417 files (happier `apps/ui/node_modules`) took 114 s (about 650
  files/s); happier's whole install (about 1M files) took 1,157 s, 19 minutes.
  `clonefile(2)` on the same directory took 7.2 s, 16 times faster. A
  directory clone is one call. Linux reflink has no directory equivalent, so
  `cp --reflink=always -R` keeps its per-file cost there (CI only today).
- **yarn 1, fixture (a, b, c, d, e, f, g, h), vouch alone**: both `check`
  commands accept the faithful clone (A) and refuse the manifest drift (F);
  `--verify-tree` also refuses out-of-range (C) and missing (D). They ACCEPT
  in-range drift (B: ms 2.1.2 against a lockfile saying 2.1.3), a stray
  directory (E), a symlink out (G) and an old-mtime edit (H). In the full
  voucher, B and E are caught by `newer_than` only when their mtime is newer
  than the record, G by the new symlink refusal, and H by nothing (the same
  limit pnpm documents). Each `check` costs about 1 s on a fixture.
- **yarn 1 on happier's own tree** (the control): both checks fail on the
  source itself (`Top level patterns don't match`; `typescript-language-server
  not installed`). `node_modules/.yarn-integrity` is from 2026-10-08 and
  `yarn.lock` from 2026-11-12, so the tree is stale against its lockfile and
  the refusal is correct, but it shows that a worktree installed before its
  last lockfile change never reuses, and falls back to install.
- **npm `install --dry-run --offline --ignore-scripts --json`**: exit 0 for
  every mutation except manifest drift (exit 1, JSON `{"error"}`), so the exit
  code cannot decide. The JSON does: `change` lists B (ms 2.1.2 to 2.1.3), `add`
  lists D, `remove` lists E, `change` lists G (same version, different path);
  `package-lock.json` stayed byte-identical in every case; only H (old-mtime
  edit) is invisible. stdout carries plain-text lines (`change ms 2.1.2 =>
  2.1.3`) BEFORE the JSON, so the helper must skip to the first `{`, and
  stderr carries an `npm warn` line: the stdout-only capture is required.
  1.1 to 2.6 s on a fixture.
- **Disk, measured on a private 4 GB APFS volume** (nothing else writing;
  `df` on the shared data volume moved by 0.7 GB with nothing cloned, so it
  cannot be used here) over happier's `apps/ui/node_modules` (1.3 GB, 74,417
  files): a plain `cp -R` (the old silent fallback) adds 1,302,676 KB and takes
  463 s; `cp -cR` adds 27,768 KB (2.1%) in 102 s; `clonefile(2)` adds 27,744 KB
  (2.1%) in 9.8 s. The clone's cost is directory and inode metadata, about 27
  MB per 74k files, and deleting the clones returned the volume to within 244
  KB of the base. Both clones are far under the 10% bar; the full copy is 100%.
- **`clonefile(2)` failure and fidelity** (happier's `apps/ui/node_modules`,
  74k files, and small trees): an unreadable file mid-tree (`chmod 000`)
  fails with `EACCES` and creates NO destination. A compressed file, an
  extended attribute, and relative and absolute symlinks all survive with
  the same checksum, xattr value and `readlink` text, with flags 0 and with
  `CLONE_NOFOLLOW`. `CLONE_NOFOLLOW` matters only when the source path itself
  is a symlink: flags 0 clones the directory it points to, `CLONE_NOFOLLOW`
  clones the link, which `link_into` then refuses (it points into the working
  tree), as `cp -R` behaved. `SIGKILL` at 0.3, 1.0 and 2.0 s into a clone
  that takes 2.7 s warm (9.8 s cold) left a COMPLETE clone each time (74,663
  files): the kernel finishes the call, so a kill never leaves a partial
  tree, only a whole clone in the temp snapshot, the kill-leftover class that
  already exists. Cloning onto that leftover returns `EEXIST`.
- Not measured: npm's trust of its hidden lockfile after a clone (e), workspace
  discovery (c), and a real npm repo with an installed tree (application-landscape
  moved to pnpm; fredericrous.com has no `node_modules`).

Consequences for the phases: Phase 1a becomes `clonefile(2)` on macOS (an
`extern "C"` declaration, no new crate) with the existing `cp` path only for
Linux and `EXDEV`/`ENOTSUP` as the refusal; the yarn gate must be judged on
the whole voucher (vouch plus `newer_than` plus the symlink refusal), not on
`check` alone; npm is the stronger voucher and goes first.

## Design

Split `reuse` (snapshot_prep.rs:791) into a shared shell and a per-manager
`Voucher`. The shell keeps everything that is already manager-neutral and
tested: lockfile byte-equality, the source-side `newer_than` scan before any
copy, `clone_dir`, `link_into`, rollback of every cloned dir on any refusal.
The voucher supplies four things:

| | pnpm (existing) | npm | yarn 1 |
|---|---|---|---|
| install record | `.modules.yaml` | `.package-lock.json` | `.yarn-integrity` |
| skipped by `newer_than` | `.bin .cache .vite*`, `.pnpm/lock.yaml`, `.pnpm-workspace-state*` | `.bin .cache .vite*`, the record | `.bin .cache .vite*`, the record |
| members | `pnpm ls -r` | `npm query .workspace` | `yarn --silent --non-interactive workspaces info --json` |
| vouch | layout check + `install --frozen-lockfile --offline` | `npm install --dry-run --offline --ignore-scripts --json` in the snapshot: accept only when added/removed/changed are all empty (npm's reify-output keys) and `package-lock.json` is byte-identical afterwards | `yarn --non-interactive check --integrity` then `yarn --non-interactive check --verify-tree` (Phase 0 confirms `--offline` does nothing for `check`) |

Output that is JSON (npm dry-run, `npm query`, `yarn workspaces info`) is reduced to plain lines by one `node -e` helper, because greengate-runtime has no JSON parser; output that does not parse is a refusal. Plumbing: run npm/yarn through a new stdout-only bounded capture, `capture_stdout_within` in `crates/greengate-runtime/src/hooks/common.rs`, and keep its exit status (no shell pipe, which would hide it); then run `node` (resolved with `hooks::common::program`, `strip_git_env`, same timeout) as a second bounded child, handing it the captured stdout on node's own piped stdin, written from a greengate thread (a `BrokenPipe` write is ignored; node's exit status decides), so no temp file exists to leak on a kill; the manager's stdin stays null. A non-zero manager exit, a missing `node` (the refusal reads `node not found on PATH (needed to read <tool>'s output); running <tool> <verb> instead`, and a test asserts it), a helper failure, empty stdout or output that does not parse is a refusal that falls back to install with every clone removed; empty stdout is never read as "no changes".

Additional refusal for npm and yarn (flat layout, so `pnpm_layout_problem`
does not apply): any symlink in the clone that resolves outside the
snapshot, `.bin` entries included. That catches `npm link` / `yarn link` and
absolute links; `link_into` only catches links back into the source. Not
applied to pnpm, so the symlink refusal leaves pnpm unchanged. It is a shared function next to `link_into`, unit-tested, with workspace links (relative, inside the snapshot) accepted.

Strays: npm's dry-run reports extraneous packages as removals. yarn 1 has no
stray detector; a stray directory created after the install is caught by
`newer_than`, one that kept an old mtime is not. That is the same class as
the existing "edited in place" limit, and the docs say so.

Out of scope, stays install-only with its refusal text: yarn berry (PnP has
no `node_modules`; `nodeLinker: node-modules` could follow with its own
`.yarn-state.yml` voucher) and bun.

No new setting. `reuse` stays opt-in and `install` stays the default, since
the in-place-edit scan still rests on timestamps.

If Phase 0 measures an npm or yarn vouch above 1 s, print `snapshot: checking node_modules in <unit> with <tool>` before it runs (npm and yarn only; pnpm output unchanged).

## Phases

### Phase 0 — measure before building

Files: none changed (scratch dir); reads `crates/greengate-runtime/src/snapshot_prep.rs`

Scratch dir, no repo change; the cases mirror those in
`crates/greengate-runtime/src/snapshot_prep.rs`. With
npm 11.9 and yarn 1.22.22, on a small fixture and on a real repo of each
kind (for yarn 1, happier; for npm, a root `package-lock.json` repo such as fredericrous.com or agent-console, since application-landscape, whose peer-range conflicts sank `npm ls`, moved to pnpm and its fixture can be rebuilt in a scratch dir if the peer-conflict case is wanted; a fresh install of happier costs about 7 GB, so run (d)'s fresh-install disk measurement only with that much free on the volume), record per manager: (a) a faithful `cp -c` clone is accepted; (b) a
wrong-version package (out of the manifest range, and in range but not the lockfile's version), a missing package, a stray dir, an `npm link`ed
package, an unsatisfied manifest are each refused, recording for each the
exit code, the stdout verdict and the stderr text; (c) workspace member
discovery output (raw stdout); (d) wall time and extra disk (Disk section) of voucher vs `npm ci --prefer-offline` /
`yarn install --frozen-lockfile --prefer-offline`; (e) whether `cp -c`
clobbering directory mtimes makes npm distrust the hidden lockfile; (f) with the hidden lockfile trusted, whether the dry-run still reports a cloned package whose `package.json` version was edited and its mtime set back before the record. If not, npm vouches for its record, not the tree, and the docs say so; (g) DONE: `cp -c` across volumes exits 0 and silently copies; `clonefile(2)` returns `EXDEV` (Phase 0 results).
Decision gate, per manager:
- The bar, in two parts. Safety: the refusal matrix has zero false accepts.
  Benefit, either of: a clone plus its vouch takes at most 50% of the
  manager's fresh install (`npm ci --prefer-offline`, `yarn install
  --frozen-lockfile --prefer-offline`, `pnpm install --frozen-lockfile
  --prefer-offline`), median of 5, on the named real repo; or the clone adds
  at most 10% of the fresh install's extra disk (Disk section). npm and yarn
  are expected to pass on disk even when the vouch is slow, so a slow vouch
  alone does not stop them; a vouch that makes the push gate slower than
  install is reported to the person as the price of the disk.
- npm: dry-run accepts (a), refuses every (b) case and meets the bar -> go.
  If dry-run is unreliable but correct, fall back to comparing the hidden
  lockfile with the committed one through the `node -e` helper. If neither
  meets the bar, stop npm and record why.
- yarn 1: same bar for `yarn check`, which yarn's docs call deprecated and "historically buggy"
  (classic.yarnpkg.com/en/docs/cli/check). Add to (b) a wrong-version nested package, a
  workspace fixture and a yarnpkg/yarn#5840-style drift; a false accept
  of the WHOLE voucher (vouch plus `newer_than` plus the symlink refusal)
  stops yarn; `check` alone is known to miss in-range drift, strays and
  links (Phase 0 results), which the other two layers must catch.
A manager that fails its gate keeps today's "always installs" behaviour and
its doc text; the other still ships. pnpm is measured and reported only: failing the bar does not remove its reuse.

### Phase 1a — `clone_dir` without its plain-copy fallback, then Phase 1b — voucher seam and JSON plumbing (`crates/greengate-runtime/src/snapshot_prep.rs`)

The refactor changes no behaviour; the disk change is `clone_dir` as the Disk section describes (`clonefile(2)` on macOS, no plain-copy fallback anywhere), for every manager including pnpm, with
the new refusal text and the reuse line, which keeps its existing prefix `node_modules in <unit> reused from the working tree` and appends `(copy-on-write clone, shares disk with the working tree)`.

Files: `crates/greengate-runtime/src/snapshot_prep.rs`, `crates/greengate-runtime/src/hooks/common.rs`, `crates/greengate/tests/it/snapshot_deps.rs`, `.github/workflows/ci.yaml`

Phase 1a: the `clone_dir` change needs only Phase 0 (g), which is done; it is its own commit and can be released alone, whatever Phase 0 finds for npm and yarn. It carries: `clone_dir` rewritten as above (one `unsafe` call, a safety comment, and unit tests that an existing destination is refused and left untouched, and that a tree holding a non-UTF-8 file name clones intact); the reuse line and refusal text; the pnpm copy-fallback tests it edits, named in the commit; and the CI change its Linux tests need (the reflink mount, with its throwaway proof run first). It does not touch `hooks/common.rs` or the vouchers.

Phase 1b: no behaviour change; starts only when Phase 0 passes npm or yarn. It adds the shared JSON plumbing both vouchers use, so either manager can ship alone: `capture_stdout_within` in `crates/greengate-runtime/src/hooks/common.rs` (unit-tested: stderr never reaches the parsed text, and a child that writes only to stderr and exits 0 is a refusal, not "no changes") and the `node -e` helper, fed on its piped stdin (no temp file; greengate installs no signal handler, `capture.rs:18`). Extract the shell
from `reuse` and give it one seam, `fn voucher(u: &Unit) -> Result<Voucher,
String>`: a plain struct holding the record name, the skip list, how members
are found, the vouch kind and the tool name; berry and bun are its `Err`
(berry is a `Unit` flag, not a `Manager` variant), and a later berry
`nodeLinker` voucher would be one more arm. Not a trait: `Manager` already
selects by `match` at `manager_cmd`, `install` and `install_verb`.
Parameterise `newer_than` with the record name and skip list, and take the
tool name for every refusal string from the voucher: today `no pnpm install
record` and `an in-place edit pnpm does not check` (snapshot_prep.rs:844,
851) are hard-coded. pnpm tests in `snapshot_deps.rs` must pass untouched, except the named copy-fallback tests.

### Phase 2 — npm voucher (`crates/greengate-runtime/src/snapshot_prep.rs`, `crates/greengate/tests/it/snapshot_deps.rs`)

Files: `crates/greengate-runtime/src/snapshot_prep.rs`, `crates/greengate/tests/it/snapshot_deps.rs`

Per the table, plus the outside-the-snapshot symlink refusal, using Phase 1's
`capture_stdout_within` and helper. Replace the `Manager::Npm` early return. The refusal reason comes from the parsed dry-run JSON (`npm would add N, remove N, change N: <first name@version>` or `package-lock.json would change`), never from `last_line` of the JSON.

### Phase 3 — yarn 1 voucher (`crates/greengate-runtime/src/snapshot_prep.rs`, `crates/greengate/tests/it/snapshot_deps.rs`, `.github/workflows/ci.yaml`)

Files: `crates/greengate-runtime/src/snapshot_prep.rs`, `crates/greengate/tests/it/snapshot_deps.rs`, `.github/workflows/ci.yaml`

Berry keeps an early return, with a message that names nodeLinker/PnP
instead of "cannot check". The yarn acceptance line says what was checked
(`yarn check --integrity` and `--verify-tree`), not "against the lockfile":
`--verify-tree` compares manifest ranges, so Phase 0's in-range-drift case
decides whether yarn ships at all. Add `yarn@1` to CI's `npm i -g` line so
the real yarn test runs there.

### Phase 4 — docs and release (`docs/configuration.md`, `docs/custom-checks.md`, `CHANGELOG.md`)

Files: `docs/configuration.md`, `docs/custom-checks.md`, `CHANGELOG.md`

Rewrite the `reuse` bullet in
`docs/configuration.md` (lines ~537-600: drop "npm is never reused" and
"Neither is yarn", add per-manager vouch, the yarn timestamp limit, berry and
bun exclusions); `docs/custom-checks.md:342` mentions reuse; CHANGELOG entry;
rustdoc at the top of `snapshot_prep.rs` and on `reuse`. Release greengate
4.6.0 (4.5.0 is already out; new capability, minor) through the `tag-release` skill after the PR
merges.

## Tests

`crates/greengate/tests/it/snapshot_deps.rs` already has a stubbed-manager
harness (`stubs_dir`, `ctl`, `set`, `stub_log`) and `real_repo(manager)` that
skips when the tool is absent.
- Replace `npm_reuse_always_installs` (line 664) and `yarn_reuse_always_installs`
  (line 571) with reuse-accepts / reuse-refuses cases mirroring the pnpm ones:
  keeps an accepted clone, rolls back whole on refusal, different lockfile
  means install, file edited after install, link outside the snapshot, nested
  lockfile is its own unit, untouched unit not prepared.
- Keep berry and bun "always installs" tests.
- Every npm/yarn refusal test asserts the printed line names the right tool and the reason, and contains no `pnpm`.
- Add `real_npm_accepts_a_matching_clone_and_refuses_a_mismatch` and the yarn
  twin next to `real_pnpm_accepts_a_matching_clone_and_refuses_a_mismatch`
  (line 1370).
- CI: add `yarn@1` to the `npm i -g` line in `.github/workflows/ci.yaml` (~1011); the ubuntu leg of the `test` job loop-mounts a reflink filesystem (xfs with `reflink=1`, or btrfs) and points `TMPDIR` at it, so the pnpm reuse tests (snapshot_deps.rs:741-930, 1370) still clone there; the CI reporter forbids skipped cases (ci.yaml:766-772), so no accept test skips; one test placed outside that mount (ext4) asserts the no-copy-on-write refusal (Disk section's text) and a successful install; the real npm/yarn twins fail instead of skipping when `GREENGATE_REAL_JS=1`, set only in the `test` job that installs them (the `windows` job runs `cargo test` with `CI=true` and has no pnpm, yarn or clone tool).

## Verification

1. `cargo test -p greengate --test it snapshot_deps`: every npm/yarn accept and refuse case listed in Tests passes; `real_npm_*`/`real_yarn_*` run, not skip (npm 11.9 and yarn 1.22.22 present); the pnpm cases pass, the named copy-fallback tests excepted.
2. Phase 0's table re-run against the built binary: each refusal case prints
   its reason and falls back to install; each acceptance prints
   `node_modules in <unit> reused from the working tree`.
3. happier (yarn 1) and one small npm repo (fredericrous.com) with `greengate.snapshotDeps reuse`:
   `greengate rehearse --wait`, confirm the reuse line and that the stamped
   run passes, then repeat with an edited file under `node_modules` and a
   changed lockfile to see the fallback.
4. Disk: on the real repos, free space on the data volume before and after
   a reused snapshot against a fresh install, per manager, three runs each;
   for npm and yarn the clone's extra bytes are at most 10% of the install's
   (pnpm is reported only). A stub `cp` cannot test the refusal, since `clone_dir` runs `/bin/cp` by absolute path and macOS no longer uses it: the refusal is covered by a unit test (existing destination: `EEXIST`, nothing created), by the Linux ext4 test (Tests) and by one manual cross-volume run on a scratch APFS image (`hdiutil create -size 64m -fs APFS`), where the unit must install and no bytes may be copied. The pnpm tests that expected a
   fallback copy are changed, and the change is named in the PR.
5. On each real repo, median voucher time against install time over 5 runs, within Phase 0's 50% bar, recorded in the PR description, plus the cost of a refused reuse (clone, refusal, then the full install).
6. `git config greengate.snapshotDeps reuse` set globally is honoured by a push in a repo that sets nothing itself (the reuse line prints).

## Risks

- npm and yarn have version-specific behaviour (hidden-lockfile trust rules,
  `yarn check` slowness). Phase 0 pins the versions tested and the docs name
  them.
- The vouch is weaker than `npm ci` for in-place edits (timestamp-based),
  exactly as for pnpm; that is why `reuse` stays opt-in.
- Spawning `npm`/`yarn` in a snapshot must keep using `manager_cmd`
  (`strip_git_env`, `SNAPSHOT_ENV`) and the bounded runner so the
  bounded-run and slot rules still apply. `capture_within` merges stdout and
  stderr (`hooks/common.rs:1772`), so every JSON-reading call (npm dry-run,
  `npm query`, `yarn workspaces info`) uses a bounded capture that keeps
  stdout on its own: parse stdout only, quote stderr in the reason, and
  treat output that will not parse as a refusal. That capture does not
  exist yet: `run_observed` (`hooks/common.rs:1393`) feeds both pipes to one
  callback. Phase 1 adds `capture_stdout_within` there, unit-tested so that
  stderr never reaches the parsed text.

## After release

For 14 days, count reuse accepts and refusals per manager from the push-gate output, and re-run 5 accepted pushes after a fresh install. One stamped pass that fails after a fresh install rolls that manager back to install-only in 4.5.1.

## Non-goals

Yarn berry and bun reuse; changing the `install` default; a new setting;
cleaning greengate's test temp dirs. pnpm behaviour changes only in two
disk-motivated ways: no plain-copy fallback in `clone_dir`, and the clone
reported in words; its vouch and refusals stay as they are.

## Code review

- amont: backend, lang:rust, tui, unix

Left out: architect and po (no new module; one new pub fn, `capture_stdout_within`, in the shared runner `hooks/common.rs`, which architect reviewed in rounds 1-3; otherwise a refusal path behind an existing opt-in setting). tui stays: a new `checking node_modules` line, a reworded yarn acceptance line and the `cloned, copy-on-write` wording are person-facing.

<!-- panel: repos=amont adds= reviewers=backend,lang:rust,unix,tui,po,architect body-sha=ee6f3bf135f8 -->
