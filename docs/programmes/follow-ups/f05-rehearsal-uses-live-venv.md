+++
type = "task"
mode = "afk"
status = "open"
blocked_by = []
repos = ["amont"]
+++
# A Python push rehearsal tests against the live checkout's venv

## Question

A push rehearsal runs a Python project's tests in a snapshot of the pushed
tree, but it uses the live checkout's `.venv`. That venv installs the project
editable (a `.pth` pointing at the live checkout's `src`), so the tests import
the live code, not the pushed code. A branch test that imports a new symbol
fails at land. This happened on 2026-11-19 on sre-agent `prog/cve-lane/t18-t19`:
8 tests red, every one on a new symbol. Today the workaround is
`greengate run --stamp` in the worktree, then land. How should the rehearsal
make the snapshot's own `src` win?

## Look at

- `crates/greengate-runtime/src/rehearsal.rs`: how the snapshot is set up, and
  which environment the test gates inherit.
- How the Python test gate finds its interpreter and venv (`$VIRTUAL_ENV`,
  `.venv`).

## Answer looks like

A rehearsal of a branch whose tests import a symbol that exists only on that
branch passes. It either puts the snapshot's `src` first on `PYTHONPATH` or
uses a venv built from the rehearsed tree. An integration test covers an
editable install whose `.pth` points at another checkout.
