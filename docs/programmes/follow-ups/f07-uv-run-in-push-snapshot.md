+++
type = "task"
mode = "afk"
status = "open"
blocked_by = []
repos = ["amont"]
+++
# `uv run` in a push snapshot runs the wrong pytest

## Question

Under `greengate.testPushedTree`, `pre-push-pytest` probes
`uv run --no-sync pytest --version` in the source checkout, where it passes
against the checkout's `.venv`, but then runs the suite with the snapshot as
its working directory. There, `uv run` finds the snapshot's `pyproject.toml`,
makes an empty `.venv` in the snapshot, and runs whatever `pytest` is first on
`PATH`. Measured on 2026-11-21 in a fresh sre-agent worktree: "Creating
virtual environment at: .venv", then a `PATH` pytest 8.4.2 refused the
project's `--cov` options (exit 4). In a rehearsal the probe itself runs in
the snapshot: for sre-agent it failed the same way and the resolver fell
through to the main worktree's `.venv`, but a project whose options a `PATH`
pytest accepts would pass the probe and run on the wrong interpreter there too.
How should the suite reach the environment the probe found?

## Look at

- `crates/greengate-runtime/src/hooks/python_tools.rs`: `resolve_python_tool`
  (probe in `root`) against `pytest` (run in `where_`).
- `UV_PROJECT_ENVIRONMENT`, or resolving the tool to the venv's own
  `bin/pytest` once the probe passes.

## Answer looks like

Under `testPushedTree`, with `uv` on `PATH`, the suite runs with the source
checkout's environment, and f05's snapshot `PYTHONPATH` still applies. An
integration test with a `uv` shim on `PATH` shows it.
