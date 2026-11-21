+++
type = "task"
mode = "afk"
status = "open"
blocked_by = []
repos = ["amont"]
due = "2026-12-05"
+++
# The venv audit route sends private package names to PyPI

## Question

ADR-0055 says private-registry packages are never sent. The uv.lock route (PR #498) now filters them. The venv route still runs `pip-audit --path <site-packages> --skip-editable`, which looks up every installed non-editable package on PyPI by name, private ones included (sre-agent's `platform-kernel`, for example).

## Look at

- `crates/greengate-runtime/src/hooks/audit.rs`: `python_run`'s venv branch, and `scan_uv_lock` / `is_pypi_source` from the lock route.
- Whether pip-audit can take a requirements list built from the installed tree minus non-PyPI names, or whether `uv.lock` can name the private packages to leave out.

## Answer looks like

On a repository with a private package installed in its `.venv`, the venv route never sends that name, which a shim recording pip-audit's input proves. It is counted in the same "private/non-PyPI package skipped" line.
