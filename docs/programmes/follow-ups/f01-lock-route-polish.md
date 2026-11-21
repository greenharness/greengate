+++
type = "task"
mode = "afk"
status = "closed"
blocked_by = []
repos = ["amont"]
+++
# The uv.lock audit route runs an export it may not need

## Question

Implementation review of greengate PR #498 left these, all deliberate there:
- On a release, `python_lock_run` runs `uv_production_names` (one `uv export --no-dev`) for every lock directory before any batch is audited, even when no batch finds anything. It also runs after the time budget is spent.
- "Next: check the network, then push again" follows any pip-audit exit 2, including non-network failures. The budget-spent and read-failure messages name no next step.
- `std::fs::canonicalize` of the private directory runs once per `scrub` call.
- The `unique_name` doc comment in `crates/greengate-runtime/src/hooks/common.rs` has one over-long line.

## Look at

- `crates/greengate-runtime/src/hooks/audit.rs`: `python_lock_run`, `audit_uv_lock`, `shipped_only`, `Budget`, `scrub_private`.
- `docs/plans/2026-11-21-audit-python-reads-uv-lock.md`, the Implementation review section.

## Answer looks like

The production names are computed lazily, only when a batch has findings and the budget is not spent. Each warning's Next line says what to run. The canonical path is computed once. Tests in `crates/greengate/tests/it/audit.rs` still pass.

## Resolution

On a release, each lock directory asks `uv export --no-dev` at most once, and
only when a batch has vulnerability rows to split and the budget is not spent.
Out of time, every finding counts and one warning says so. The venv route asks
only when it has findings too. pip-audit's Next line names the network only
when its reason does, else `pip-audit --version`. The budget-spent warnings
name `greengate.timeout`, and the scratch-file failures name `$TMPDIR`. The
private directory is canonicalised once per lock directory.
