+++
type = "task"
mode = "afk"
status = "closed"
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

## Resolution

The snapshot's own import roots now go first on `PYTHONPATH` when
`pre-push-pytest` runs in a snapshot (a rehearsal, or `testPushedTree`). They
are read from the virtualenvs the gate can have borrowed (`$VIRTUAL_ENV`, the
checkout's `.venv`, the main worktree's, the rehearsal source's): each absolute
path line of their `.pth` files, taken relative to the checkout that owns the
venv, is mapped into the snapshot when the snapshot has that directory. So a
`src/` layout, a flat layout and a workspace member all map, without a rule per
layout. `PYTHONPATH` comes before site-packages, and before setuptools'
editable finder, which is appended to `sys.meta_path`; an inherited
`PYTHONPATH` is kept after the new roots. Paths are canonical before they are
compared (`/var/folders` is `/private/var/folders`).

Two integration tests use a real venv whose `.pth` names the live checkout's
`src`, with a working tree that lacks the symbol the pushed commit adds:
`pytest_gate::the_pushed_trees_src_wins_over_an_editable_install`
(`testPushedTree`) and
`rehearsal::a_rehearsal_imports_the_snapshots_src_not_the_editable_install`
(the 2026-11-19 shape: the snapshot borrows the main worktree's venv). Both
fail on the old code with `module 'pkg' has no attribute 'NEW'`.

Not covered: the `uv run --no-sync` branch under `testPushedTree`. The probe
runs in the source checkout, but the suite runs in the snapshot, where `uv run`
makes an empty `.venv` and runs whatever `pytest` is on `PATH`. Filed as
[f07](f07-uv-run-in-push-snapshot.md).
