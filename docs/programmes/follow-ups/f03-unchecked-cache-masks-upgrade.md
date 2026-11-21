+++
type = "task"
mode = "afk"
status = "closed"
blocked_by = []
repos = ["amont"]
+++
# A cached "unchecked" audit masks a greengate upgrade for an hour

## Question

On 2026-11-21 at 13:54Z, greengate 4.12.0 printed only "audit-python could not complete" on sre-agent's live checkout. The cause was an `unchecked` record in `refs/notes/greengate-audit`, written at 13:12Z by greengate 4.11.0, which could not audit a uv project without a venv. A record that checked nothing answers for an hour (`UNCHECKED_MAX_AGE_SECS`). So the new route, which would have answered, did not run until 14:12Z. In a fresh clone with no notes, 4.12.0 audited correctly.

## Look at

- `crates/greengate-runtime/src/hooks/audit.rs`: `AUDIT_REF`, `AUDIT_FORMAT`, `UNCHECKED_MAX_AGE_SECS`, and where a record is read and replayed.
- What the record keys on today (the lockfiles' content), and whether the binary version or the route taken belongs in the key.

## Answer looks like

After an upgrade, an `unchecked` record from an older greengate is not replayed. Either the key or the format carries the version, or an unchecked record is never reused across versions. A test writes an unchecked record with an older version and shows the audit runs.

## Resolution

A record's header now ends with the greengate version that wrote it
(`greengate-audit-v1 <at> <class> <waivable> <tool> <version>`). An
`unchecked` record replays only when that version is the running one, so a
record from an older binary, or one with no version (written before this
change, like sre-agent's 4.11.0 record), is owed again on the next push. The
key and `AUDIT_FORMAT` are unchanged: a format bump would have voided every
clean record on every machine and fixed only this one upgrade. Clean records
and records with findings still answer across versions until their age runs
out. An older greengate reading a new record ignores the extra field.

## Verification

- `cargo test -p greengate --test it push_gates`: the three new tests pass.
  `an_unchecked_record_from_an_older_greengate_is_not_replayed` writes an
  unchecked record offline, rewrites its version (dropped; `4.11.0`), then
  pushes online with inline gates: the tool runs again (1 → 2 runs) and no
  replay line is printed. With the version check disabled it fails (the
  record replays, 1 run). Control: a same-version unchecked record replays;
  a clean record with no version still replays.
