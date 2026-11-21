# audit-python reads uv.lock: full reviews


**Round 1** (opus; tokens / wall time): backend 29k / 26 s, rust 34k / 30 s,
po 28k / 28 s, architect 34k / 36 s, tui 29k / 27 s, unix 44k / 64 s. All
approve-with-changes, and all 29 findings accepted.
- Temp file (backend, rust, architect, tui, unix): a predictable pid name,
  and no cleanup mechanism. Merged into step 1.
- Per-directory locks (architect high), with a timeout (backend).
- An empty filtered export read as Clean (rust high, backend).
- Flag sniffed from argv (architect, rust). Fixtures for the lock scan
  (rust). The `-o`/stdout shim (rust).
- Release effect on sre-agent (po high). Exact live numbers, Measure, and
  follow-up tickets (po, architect).
- Warning gives the next command (tui high). Two skip messages, distinct
  (tui high). Source line, path never shown, failure clipped (tui).
- Shim flag order (unix).

**Round 2** (opus): backend 41k / 24 s, tui 34k / 12 s, po 39k / 19 s,
rust 41k / 25 s, architect 41k / 23 s, unix 48k / 15 s. All
approve-with-changes. No blocker or high, so every finding is carried into
implementation:
1. The live check's string must match step 4, with the singular and plural
   ("1 private/non-PyPI package skipped (platform-kernel)"). Raised by
   backend, tui, po, rust, architect and unix.
2. Results across several lock directories are the worst directory's: an
   Unavailable is never hidden by a Clean. `uv_production_names` takes the
   directory, runs through `audited_in`, and the results merge as in
   `audit_each_dir`. Lines carry a lock-path prefix when the lock is not at
   the root. Raised by backend, rust, architect, unix and tui.
3. The Measure counts live where quiet mode does not hide them (po,
   backend).
4. Clip `failure_line` to 60 characters so the line fits 80 columns, and
   the export-failure line says "did NOT run" (rust, backend).
5. The mirror case is a known gap and says "did NOT run" (po, unix).
6. The sre-agent ticket is due by 2026-11-27 (po).
7. "At least 10" findings under `--all-groups`, any extra explained as
   dev-only (po).

**Codex** (`codex exec`, high effort): approve-with-changes, with 2 highs
applied.
- Markers are stripped, and versions are batched.
- `--all-packages` is added to both exports.
- Carried into implementation: create the directory with `DirBuilder` mode
  0700, tested under umask 000; add `--no-default-groups` to the production
  export, tested with a default `lint` group.

**Backend re-run** (final body, 44k / 12 s): approve. Carried into
implementation: one time budget covers the whole lock route, and batches
left unrun when it runs out are Unavailable.

