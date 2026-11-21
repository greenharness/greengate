//! The Python test gate: which pytest answers. A fake pytest on a shimmed
//! PATH stands in for the suite; a green, a red and an untouched push are rows
//! of the tables in `tool_tables`.
#![cfg(unix)]

use crate::common;
use common::Repo;

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::process::Stdio;

fn shim(r: &Repo, body: &str) {
    let dir = r.path(".git/toolshims");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let p = dir.join("pytest");
    std::fs::write(&p, format!("#!/bin/sh\n{body}")).expect("write");
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).expect("chmod");
}

/// A pytest inside the project's own `.venv`, which is where a Python project
/// keeps the interpreter that can actually import it.
fn venv_shim(r: &Repo, body: &str) {
    let dir = r.path(".venv/bin");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let p = dir.join("pytest");
    std::fs::write(&p, format!("#!/bin/sh\n{body}")).expect("write");
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).expect("chmod");
}

fn push(r: &Repo, from: &str) -> (i32, String) {
    let head = String::from_utf8_lossy(&r.git(&["rev-parse", "HEAD"]).stdout)
        .trim()
        .to_string();
    let line = format!("refs/heads/feat/x {head} refs/heads/feat/x {from}\n");
    let path = format!(
        "{}:{}",
        r.path(".git/toolshims").display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let mut child = crate::common::amont_cmd()
        .arg("--hooks-dir")
        .arg(r.path(".git/hooks"))
        .arg("pre-push-pytest")
        .current_dir(&r.dir)
        .env("PATH", path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(line.as_bytes())
        .unwrap();
    let out = child.wait_with_output().expect("wait");
    (
        out.status.code().unwrap_or(-1),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

fn repo() -> (Repo, String) {
    let r = Repo::new();
    r.stage("conftest.py", "# pytest setup\n");
    r.commit("chore: base");
    let base = String::from_utf8_lossy(&r.git(&["rev-parse", "HEAD"]).stdout)
        .trim()
        .to_string();
    r.stage("test_x.py", "def test_x():\n    assert True\n");
    r.commit("feat: python change");
    (r, base)
}

/// The project's own `.venv/bin/pytest` beats whatever is first on `PATH`.
///
/// This gate used to resolve with a bare `which("pytest")`, unlike `ruff` and
/// `pyright` beside it, which go through `resolve_python_tool`. A project's
/// tests import the project, and a `PATH` pytest belongs to whatever
/// interpreter happens to be first there — in a `src/` layout it cannot see
/// the package at all. The gate then reported `ModuleNotFoundError` as a test
/// FAILURE and blocked the push, in a repository whose suite passes under
/// `uv run pytest`. A gate that cannot tell "your tests broke" from "I ran the
/// wrong interpreter" is worse than no gate.
#[test]
fn the_projects_own_venv_beats_whatever_is_first_on_path() {
    let (r, from) = repo();
    // The one on PATH would fail the push, and says so if it is ever reached.
    shim(&r, "echo FROM-PATH-PYTEST; exit 1\n");
    // The one that belongs to the project passes.
    venv_shim(&r, "echo FROM-VENV-PYTEST; exit 0\n");

    let (code, out) = push(&r, &from);
    assert!(
        out.contains("FROM-VENV-PYTEST"),
        "the venv's pytest must be the one that runs: {out}"
    );
    assert!(
        !out.contains("FROM-PATH-PYTEST"),
        "the PATH pytest must not be reached: {out}"
    );
    assert_eq!(code, 0, "and its green result is the verdict: {out}");
}

/// A real virtualenv, the way `uv sync` leaves a `src/` project: the package
/// installed EDITABLE, a `.pth` in site-packages naming `<checkout>/src`.
/// `pytest` is a script that imports the project with that venv's interpreter
/// and fails unless the symbol only the pushed commit has is there.
pub(crate) fn editable_venv(r: &Repo) {
    let made = std::process::Command::new("python3")
        .args(["-m", "venv", "--without-pip"])
        .arg(r.path(".venv"))
        .output()
        .expect("python3 -m venv");
    assert!(made.status.success(), "venv: {made:?}");
    let lib = r.path(".venv/lib");
    let site = std::fs::read_dir(&lib)
        .expect("venv lib")
        .flatten()
        .find(|e| e.file_name().to_string_lossy().starts_with("python"))
        .expect("lib/python*")
        .path()
        .join("site-packages");
    std::fs::write(
        site.join("_pkg.pth"),
        format!("{}\n", r.path("src").display()),
    )
    .expect("pth");
    common::fixture_exec(
        &r.path(".venv/bin/pytest"),
        "#!/bin/sh\n\
         exec \"$(dirname \"$0\")/python\" -c 'import pkg; pkg.NEW; print(\"IMPORTED\", pkg.__file__)'\n",
    );
}

/// The pushed tree's own `src` wins over the editable install's `.pth`.
///
/// A push snapshot (`greengate.testPushedTree`, and every rehearsal) has no
/// virtualenv of its own, so the gate runs the source checkout's — whose
/// `.pth` points at the SOURCE checkout's `src`. The suite then imported the
/// code on disk, not the code being pushed: on 2026-11-19 a branch whose tests
/// used eight new symbols failed at land, every test on a symbol the working
/// tree had not been switched to yet. Here the working tree lacks `NEW`, the
/// pushed commit has it, and the push must pass.
#[test]
fn the_pushed_trees_src_wins_over_an_editable_install() {
    if common::missing("python3") {
        return;
    }
    let r = Repo::new();
    r.stage("conftest.py", "# pytest setup\n");
    r.stage("src/pkg/__init__.py", "OLD = 1\n");
    r.commit("chore: base");
    let from = String::from_utf8_lossy(&r.git(&["rev-parse", "HEAD"]).stdout)
        .trim()
        .to_string();
    r.stage("src/pkg/__init__.py", "OLD = 1\nNEW = 1\n");
    r.stage("tests/test_new.py", "from pkg import NEW\n");
    r.commit("feat: a new symbol");
    // The working tree is behind the commit being pushed.
    r.write("src/pkg/__init__.py", "OLD = 1\n");
    r.git(&["config", "greengate.testPushedTree", "true"]);
    editable_venv(&r);
    // No `uv` on PATH: the resolver's `.venv/bin/pytest` branch answers on
    // every machine, not `uv run` on this one and the venv on CI.
    let head = String::from_utf8_lossy(&r.git(&["rev-parse", "HEAD"]).stdout)
        .trim()
        .to_string();
    let line = format!("refs/heads/feat/x {head} refs/heads/feat/x {from}\n");
    let mut child = crate::common::amont_cmd()
        .arg("--hooks-dir")
        .arg(r.path(".git/hooks"))
        .arg("pre-push-pytest")
        .current_dir(&r.dir)
        .env("PATH", "/usr/bin:/bin")
        .env_remove("VIRTUAL_ENV")
        .env_remove("PYTHONPATH")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(line.as_bytes())
        .unwrap();
    let out = child.wait_with_output().expect("wait");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        text.contains("IMPORTED"),
        "the pushed commit's NEW must import: {text}"
    );
    assert!(
        !text.contains(&format!("IMPORTED {}", r.path("src").display())),
        "it imported the working tree, not the pushed commit: {text}"
    );
    assert_eq!(out.status.code(), Some(0), "{text}");
}
