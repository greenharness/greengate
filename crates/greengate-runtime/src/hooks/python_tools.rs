//! pre-commit-ruff and pre-commit-pyright — the two Python hooks.
//!
//! Both are scoped: they only fire when the repo opts in (a ruff/pyright config
//! or the matching `[tool.*]` table), so a JS repo never pulls a Python tool.
//!
//! Both prefer the repo's PINNED tool over an ambient latest, in the same order
//! the shell versions established: `uv run --no-sync` (the lockfile-pinned one
//! CI runs) → the worktree's .venv → the MAIN worktree's .venv (a linked
//! worktree has none of its own) → PATH → uvx (unpinned LATEST, with a warning,
//! because it flags issues the CI-pinned version does not — phantom failures).

use super::common::{
    fail, fixing_enabled, hl, ok, repo_root, restage, run as run_tool, run_quiet, staged_files,
    warn, which, Restaged,
};
use crate::check::Outcome;
use crate::git;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The extensions both Python checks consume. Exported so `registry.rs`
/// declares the scope from the same constant — see `lint_json_yaml::EXTS` for
/// the drift this prevents.
pub const EXTS: &[&str] = &[".py", ".pyi"];

fn tool_runs(root: &str, argv: &[String]) -> bool {
    let Some((p, rest)) = argv.split_first() else {
        return false;
    };
    Command::new(p)
        .args(rest)
        .arg("--version")
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn main_worktree_venv(tool: &str) -> Option<String> {
    let common = git::stdout(&["rev-parse", "--path-format=absolute", "--git-common-dir"])?;
    let p = Path::new(&common).parent()?.join(".venv/bin").join(tool);
    p.is_file().then(|| p.to_string_lossy().into_owned())
}

/// Returns (argv, warned_about_unpinned).
///
/// Every branch yields a RESOLVED path, never a bare name. `Command::new` does
/// no PATHEXT resolution, so a bare `uv`/`uvx`/`ruff` cannot execute
/// `uv.exe`/`ruff.cmd` on Windows: the spawn fails with "program not found" and
/// a `Severity::Block` check reports an installed tool as broken. That is the
/// incident `common::program` was written for, and these were the last places
/// still handing `Command` a bare name — three of them AFTER `which()` had
/// already succeeded and thrown the answer away.
fn resolve_python_tool(root: &str, tool: &str) -> Option<(Vec<String>, bool)> {
    if let Some(uv) = which("uv") {
        let argv = vec![uv, "run".into(), "--no-sync".into(), tool.into()];
        if tool_runs(root, &argv) {
            return Some((argv, false));
        }
    }
    let local = format!("{root}/.venv/bin/{tool}");
    if Path::new(&local).is_file() {
        return Some((vec![local], false));
    }
    if let Some(v) = main_worktree_venv(tool) {
        return Some((vec![v], false));
    }
    // The path `which` already resolved, rather than probing and discarding it.
    if let Some(found) = which(tool) {
        return Some((vec![found], false));
    }
    if let Some(uvx) = which("uvx") {
        return Some((vec![uvx, tool.into()], true));
    }
    None
}

fn opts_in(root: &str, configs: &[&str], table: &str) -> bool {
    if configs.iter().any(|c| Path::new(root).join(c).is_file()) {
        return true;
    }
    std::fs::read_to_string(Path::new(root).join("pyproject.toml"))
        .map(|t| t.lines().any(|l| l.trim_start().starts_with(table)))
        .unwrap_or(false)
}

pub fn ruff(settings: &crate::config::Settings, _args: &[std::ffi::OsString]) -> Outcome {
    let files = staged_files(EXTS);
    if files.is_empty() {
        return Outcome::Passed;
    }
    let root = repo_root();
    if !opts_in(&root, &["ruff.toml", ".ruff.toml"], "[tool.ruff") {
        return Outcome::Passed;
    }
    let Some((argv, unpinned)) = resolve_python_tool(&root, "ruff") else {
        warn("ruff config found but no ruff/uvx binary. Install ruff or uv.");
        return Outcome::Unavailable;
    };
    if unpinned {
        warn(&format!(
            "No pinned ruff found (.venv); using {} (latest) — may flag issues the CI-pinned ruff doesn't.",
            hl("uvx ruff")
        ));
    }

    // The registry has always declared `Fix::Rewrite` for this check, and
    // `amont list --json` reported `"fix":"rewrite"` — which `agents_md`
    // explicitly tells agents to trust — while no fixing code existed
    // anywhere. Only prettier and the manifest's externals ever called
    // `restage`. Rather than downgrade the declaration, the fixing is now
    // real.
    //
    // Repair FIRST and QUIETLY, then let the check passes below decide. Ruff
    // legitimately leaves findings it cannot fix, unlike `cargo fmt`, so the
    // repair pass's own exit code says nothing about the verdict — and running
    // it loudly would print the surviving offenders twice.
    let mut repaired = false;
    if fixing_enabled(settings) {
        let _ = run_quiet(
            settings,
            &root,
            &argv,
            &with_files(&["check", "--fix"], &files),
        );
        let _ = run_quiet(settings, &root, &argv, &with_files(&["format"], &files));
        match restage(&files) {
            Restaged::Staged => repaired = true,
            Restaged::Failed(stuck) => {
                fail(&format!(
                    "ruff rewrote these files but {} failed — the index still holds the OLD \
                     content: {}",
                    hl("git add"),
                    stuck.join(", ")
                ));
                return Outcome::Failed;
            }
            Restaged::Nothing => {}
        }
    }

    let mut failed = false;
    if !run_tool(settings, &root, &argv, &with_files(&["check"], &files)) {
        fail(&format!(
            "Ruff lint issues. Run {}. Offenders above.",
            hl("ruff check --fix")
        ));
        failed = true;
    }
    if !run_tool(
        settings,
        &root,
        &argv,
        &with_files(&["format", "--check"], &files),
    ) {
        fail(&format!(
            "Ruff found unformatted files. Run {} on the files listed above.",
            hl("ruff format")
        ));
        failed = true;
    }

    if failed {
        // A repair that could not finish the job still blocks — and whatever it
        // DID fix is already staged, so the next attempt starts from there.
        return Outcome::Failed;
    }
    if repaired {
        ok(settings, "Ruff fixed and re-staged");
        return Outcome::Fixed;
    }
    // An unpinned ruff RAN, and its verdict was clean — that is a pass, not a
    // gap. The caveat above is advice about which ruff spoke, not a claim that
    // none did.
    ok(settings, "Ruff passed");
    Outcome::Passed
}

/// `<sub…> --force-exclude -- <files>`.
///
/// `--force-exclude` on every pass so ruff honours the project's `exclude`
/// even though the paths are handed to it explicitly. `--` before the file
/// list because a staged file named e.g. `-x.py` would otherwise be read as a
/// flag by ruff's own parser.
fn with_files(sub: &[&str], files: &[String]) -> Vec<String> {
    let mut argv: Vec<String> = sub.iter().map(|s| (*s).to_string()).collect();
    argv.push("--force-exclude".into());
    argv.push("--".into());
    argv.extend(files.iter().cloned());
    argv
}

pub fn pyright(settings: &crate::config::Settings, _args: &[std::ffi::OsString]) -> Outcome {
    let files = staged_files(EXTS);
    if files.is_empty() {
        return Outcome::Passed;
    }
    let root = repo_root();
    if !opts_in(
        &root,
        &["pyrightconfig.json", "pyrightconfig.jsonc"],
        "[tool.pyright",
    ) {
        return Outcome::Passed;
    }
    let Some((argv, _)) = resolve_python_tool(&root, "pyright") else {
        warn("pyright config found but no pyright binary. Install pyright or uv.");
        return Outcome::Unavailable;
    };
    // Scoped to the STAGED files, not the whole tree: fast enough for a
    // pre-commit hook while catching the per-file errors that are the usual
    // local-vs-CI gap. Pyright still resolves the whole workspace for imports,
    // so inference is unchanged; only the reported set is scoped. CI's
    // whole-tree run stays the authority for cross-file-only errors.
    // PYRIGHT HAS NO `--` SEPARATOR, and this check used to pass one.
    //
    // Every other tool driven from this repository takes `--` to mean "flags
    // are over" — ruff, prettier, yamllint, node, yq all do, and each has a
    // regression test for a staged file named like a flag. Pyright does not.
    // It has a hand-rolled argument parser that treats `--` as an ordinary
    // path, so `pyright -- a.py` reports
    //
    //     File or directory ".../--" does not exist
    //
    // and exits 4. `run_tool` sees a non-zero status, and a check declared
    // `Severity::Block` blocked the commit. Not on bad code — on ALL code. Any
    // repository with a `pyrightconfig.json` or a `[tool.pyright]` table could
    // not commit a `.py` file at all while pyright was installed, and the
    // message it got named a file nobody had written.
    //
    // It survived because the check's only test asserted that it does NOTHING
    // in a repo with no config, which is the one path that never reaches this
    // line. `linters.rs` now has both halves — a type error must block, and
    // clean code must not — and it was the second of those that caught this in
    // its first run. A check that fails on everything satisfies "reports a type
    // error" perfectly.
    //
    // `./` in place of `--` keeps the protection that was intended. `./-p.py`
    // begins with a dot, so no argument parser can mistake it for a flag, and
    // pyright resolves it against the working directory `run_tool` already
    // sets. Git reports staged paths relative to the repository root with
    // forward slashes on every platform, so this is safe to prepend blindly.
    // `--warnings` first: exit non-zero on warning diagnostics too, not just
    // errors — see lint_js for why an unread warn list is worse than a block.
    let mut with_files: Vec<String> = vec!["--warnings".to_string()];
    with_files.extend(files.iter().map(|f| format!("./{f}")));
    if !run_tool(settings, &root, &argv, &with_files) {
        fail("Pyright type errors. Please fix");
        return Outcome::Failed;
    }
    ok(settings, "pyright passed");
    Outcome::Passed
}

/// The virtualenvs a snapshot's pytest can have borrowed: an activated one,
/// the checkout's own `.venv`, the main worktree's (a linked worktree has
/// none), and the rehearsal's source worktree's.
fn borrowed_venvs(root: &str) -> Vec<PathBuf> {
    let main = git::stdout(&["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .and_then(|common| Path::new(&common).parent().map(|p| p.join(".venv")));
    std::env::var_os("VIRTUAL_ENV")
        .map(PathBuf::from)
        .into_iter()
        .chain(std::iter::once(Path::new(root).join(".venv")))
        .chain(main)
        .chain(crate::rehearsal::source_worktree().map(|s| s.join(".venv")))
        .collect()
}

/// Every `site-packages` under `venv`: `lib/python3.X/site-packages`, or
/// Windows' `Lib/site-packages`.
fn site_packages(venv: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(venv.join("lib"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("python"))
        .map(|e| e.path().join("site-packages"))
        .collect();
    found.push(venv.join("Lib").join("site-packages"));
    found.retain(|p| p.is_dir());
    found
}

/// The directories an editable install in `site` adds to `sys.path`: the
/// absolute path lines of its `.pth` files. `import` lines (setuptools'
/// strict-mode finder, coverage hooks) carry no path and are skipped.
fn pth_paths(site: &Path) -> Vec<PathBuf> {
    let mut pths: Vec<PathBuf> = std::fs::read_dir(site)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "pth"))
        .collect();
    pths.sort();
    pths.iter()
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .flat_map(|text| {
            text.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with("import"))
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The snapshot's own copies of the directories a borrowed venv's editable
/// install points at: `<checkout>/src` becomes `<snapshot>/src`, and a flat
/// layout or a workspace member maps the same way. Each `.pth` path is taken
/// relative to the checkout that owns its venv (the venv's parent), and kept
/// only when the snapshot has that directory. Paths are canonical before
/// they are compared: snapshots live under `/var/folders`, which is
/// `/private/var/folders` once resolved.
fn snapshot_import_roots(snapshot: &Path, root: &str) -> Vec<PathBuf> {
    let Ok(snapshot_dir) = snapshot.canonicalize() else {
        return Vec::new();
    };
    let mut roots: Vec<PathBuf> = Vec::new();
    for venv in borrowed_venvs(root) {
        let Some(owner) = venv.parent().and_then(|parent| parent.canonicalize().ok()) else {
            continue;
        };
        if owner == snapshot_dir {
            continue; // the snapshot's own venv points at the snapshot already
        }
        for site in site_packages(&venv) {
            for pth_entry in pth_paths(&site) {
                let Ok(canonical) = pth_entry.canonicalize() else {
                    continue;
                };
                let Ok(relative) = canonical.strip_prefix(&owner) else {
                    continue;
                };
                let mapped = snapshot_dir.join(relative);
                if mapped.is_dir() && !roots.contains(&mapped) {
                    roots.push(mapped);
                }
            }
        }
    }
    roots
}

/// `roots`, then whatever `PYTHONPATH` already held. `PYTHONPATH` comes
/// before site-packages on `sys.path`, so these win over a `.pth`, and over
/// setuptools' editable finder, which is appended to `sys.meta_path`.
fn python_path_with(roots: &[PathBuf]) -> Option<std::ffi::OsString> {
    if roots.is_empty() {
        return None;
    }
    let inherited = std::env::var_os("PYTHONPATH");
    let tail = inherited
        .as_deref()
        .map(|v| std::env::split_paths(v).collect::<Vec<_>>())
        .unwrap_or_default();
    std::env::join_paths(roots.iter().cloned().chain(tail)).ok()
}

/// The Python test gate — `pre-push-pytest`, `cargo-test`'s shape for the
/// third ecosystem. JS had a push-time suite from the start and Rust
/// gained one; Python repositories declaring a pytest setup (a
/// `pytest.ini` or a `conftest.py` — a bare `pyproject.toml` is not a
/// promise to test) ran nothing at push until now.
///
/// Same contract as the other suites: runs against the PUSHED tree per
/// ref (see `pushed_tree`), `Unavailable` — loudly, never green — when
/// git will not answer or pytest is not installed, and fail-fast on a red
/// suite.
pub fn pytest(
    settings: &crate::config::Settings,
    refs: &[crate::pushrefs::PushRef],
    gate: &str,
) -> Outcome {
    let Some(root) = crate::git::stdout(&["rev-parse", "--show-toplevel"]) else {
        super::common::warn("pytest: git would not answer — the gate did NOT run");
        return Outcome::Unavailable;
    };
    let zero = crate::git::stdout(&["hash-object", "--stdin"])
        .map(|h| "0".repeat(h.len()))
        .unwrap_or_else(|| "0".repeat(40));
    let mut ran_any = false;
    for r in refs {
        let changed = crate::pushrefs::changed_files_for(r, &zero);
        if !changed.iter().any(|f| EXTS.iter().any(|e| f.ends_with(e))) {
            continue; // this ref pushes no Python
        }
        // `resolve_python_tool`, like `ruff` and `pyright` — NOT a bare
        // `which`, which is what this used to do and is the one thing that
        // makes a Python gate useless.
        //
        // A project's tests import the project. A `pytest` found on `PATH`
        // belongs to whatever interpreter happens to be first there, which in
        // a `src/` layout cannot see the package at all: the gate reported
        // `ModuleNotFoundError` — as a test FAILURE, blocking the push — in a
        // repository whose suite passes 1296 tests under `uv run pytest`. The
        // resolver tries `uv run --no-sync` first, then `.venv/bin/`, and only
        // then falls back to `PATH`.
        let Some((argv, _)) = resolve_python_tool(&root, "pytest") else {
            super::common::warn(
                "Python changed but pytest is not installed — the gate did NOT run",
            );
            return Outcome::Unavailable;
        };
        // Where THIS ref's suite runs decides what it is answering about —
        // the pushed commits, not whatever is open in the editor.
        let (where_, guard) = crate::pushed_tree::where_to_run(settings, &r.local_oid, &root, gate);
        let mut cmd = std::process::Command::new(&argv[0]);
        cmd.args(&argv[1..]);
        cmd.current_dir(&where_).stdin(std::process::Stdio::null());
        // A snapshot has no virtualenv of its own: the pytest above belongs
        // to the source checkout's, whose editable install points at the
        // SOURCE `src`. Put the snapshot's own import roots first, or the
        // suite tests the code on disk instead of the code being pushed.
        if crate::rehearsal::in_snapshot() || guard.is_some() {
            let roots = snapshot_import_roots(&where_, &root);
            if let Some(path) = python_path_with(&roots) {
                cmd.env("PYTHONPATH", path);
            }
        }
        super::common::strip_git_env(&mut cmd);
        if !super::common::bounded_success(settings, &mut cmd, "pytest") {
            super::common::fail("Python tests failed. Push aborted.");
            return Outcome::Failed;
        }
        ran_any = true;
    }
    if ran_any {
        super::common::ok(settings, "Python tests passed");
    }
    Outcome::Passed
}
