//! Whether each decision-bearing repository's `greentopo` session hook is current.
//!
//! `greentopo hook install` writes two tracked files — `.claude/hooks/greentopo-heads.sh`
//! and a `SessionStart` entry in `.claude/settings.json` — so that an agent
//! session opens with the repository's decision heads and adopted rules in
//! front of it. The script's bytes are its version: every greentopo release that
//! changes it leaves every repository behind until somebody re-runs the
//! command there, which is the same silent drift the `AGENTS.md` pointer had
//! before `--agents-md` existed, spread across every repository that keeps an
//! `.adr.yaml`.
//!
//! This module never renders the hook. The template is greentopo's, its notion of
//! "current" is greentopo's, and a copy here would be a second way to be stale —
//! so both questions are put to the `greentopo` binary and only its exit code is
//! read (`0` current, `1` out of date), which is the contract its `--check`
//! documents. A repository with no `.adr.yaml` is not asked at all: there is
//! no corpus for the hook to print, and `greentopo hook install --check` would
//! report it as stale.
//!
//! What git would do with the files is reported alongside, from `git
//! check-ignore`, because the failure that motivated tracking the hook was a
//! repository whose `.gitignore` excluded `.claude/` wholesale: the command
//! succeeded, the hook worked for the one person who ran it, and shipped to
//! nobody. That is a `Warning`, never a write — editing another repository's
//! `.gitignore` is a judgment (`.claude/*` plus negations, in every case seen
//! so far, but with per-repository exceptions) and not this tool's to make.

use std::path::Path;
use std::process::Command;

use serde::Serialize;

/// The registry greentopo reads. Its presence is what makes a repository a corpus.
pub const REGISTRY: &str = ".adr.yaml";
/// The program that judges and writes the session hook. aval became
/// greentopo (ADR-0072); the hook it writes is `greentopo-heads.sh`.
pub const PROGRAM: &str = "greentopo";
/// The two files `greentopo hook install` writes, spelled as it spells them.
pub const SCRIPT_PATH: &str = ".claude/hooks/greentopo-heads.sh";
pub const SETTINGS_PATH: &str = ".claude/settings.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum GreentopoHookState {
    /// No `.adr.yaml` here. Not applicable, and nothing was spawned to find out.
    NoCorpus,
    /// A corpus, and no `greentopo` on `PATH` to judge its hook with. Absence of
    /// evidence: reported, never read as "current".
    NoGreentopo,
    /// `greentopo hook install --check` exited 0.
    Current,
    /// `greentopo hook install --check` exited 1: at least one of the two files
    /// would be written.
    Stale,
    /// greentopo answered with something other than 0 or 1 — a usage error from a
    /// version that spells the subcommand differently, a corpus it will not
    /// load. Carried verbatim rather than mapped onto a state it did not say.
    Unknown { why: String },
}

/// One repository's answer, plus what git would do with the files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GreentopoHook {
    pub state: GreentopoHookState,
    /// The hook's paths that `git check-ignore` matched. Non-empty means a
    /// current hook still ships to nobody. Always empty for `NoCorpus`.
    pub ignored: Vec<String>,
}

impl Default for GreentopoHook {
    fn default() -> Self {
        GreentopoHook {
            state: GreentopoHookState::NoCorpus,
            ignored: Vec::new(),
        }
    }
}

/// Ask greentopo about `repo`. `greentopo` is the binary to run — `"greentopo"` in
/// production, a fake under test — and nothing is spawned for a repository
/// without a registry.
pub fn state(repo: &Path, greentopo: &str) -> GreentopoHook {
    if !repo.join(REGISTRY).is_file() {
        return GreentopoHook::default();
    }
    let state = match Command::new(greentopo)
        .args(["hook", "install", "--check"])
        .current_dir(repo)
        .output()
    {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => GreentopoHookState::NoGreentopo,
        Err(e) => GreentopoHookState::Unknown { why: e.to_string() },
        Ok(out) => match out.status.code() {
            Some(0) => GreentopoHookState::Current,
            Some(1) => GreentopoHookState::Stale,
            code => GreentopoHookState::Unknown {
                why: format!(
                    "greentopo hook install --check exited {}: {}",
                    code.map_or("by signal".to_string(), |c| c.to_string()),
                    first_line(&out.stderr).unwrap_or_else(
                        || first_line(&out.stdout).unwrap_or_else(|| "no output".to_string())
                    )
                ),
            },
        },
    };
    GreentopoHook {
        state,
        ignored: ignored(repo),
    }
}

/// Run `greentopo hook install` in `repo`. Returns how many files greentopo reports
/// having written, from the `wrote` lines of its own report.
pub fn install(repo: &Path, greentopo: &str) -> Result<usize, String> {
    let out = Command::new(greentopo)
        .args(["hook", "install"])
        .current_dir(repo)
        .output()
        .map_err(|e| format!("{greentopo}: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "greentopo hook install exited {}: {}",
            out.status
                .code()
                .map_or("by signal".to_string(), |c| c.to_string()),
            first_line(&out.stderr)
                .or_else(|| first_line(&out.stdout))
                .unwrap_or_else(|| "no output".to_string())
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| l.trim_start().starts_with("wrote"))
        .count())
}

/// The hook's paths git would ignore in `repo`, best effort. Exit 1 from
/// `check-ignore` is "nothing matched", the ordinary case; no git at all is
/// not this module's to report.
fn ignored(repo: &Path) -> Vec<String> {
    let Ok(out) = Command::new("git")
        .args(["check-ignore", "--", SCRIPT_PATH, SETTINGS_PATH])
        .current_dir(repo)
        .output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

fn first_line(bytes: &[u8]) -> Option<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "fleet-greentopo-hook-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A stand-in `greentopo` whose `--check` exits `check_rc` and whose `install`
    /// prints `wrote` lines. Records what it was asked in `<dir>/calls`.
    /// A `/bin/sh` script, so unix-only — the same `cfg` every executable
    /// fixture in this crate carries; the two tests below that need no fake
    /// binary run everywhere.
    #[cfg(unix)]
    fn fake_greentopo(d: &Path, check_rc: i32) -> String {
        use std::os::unix::fs::PermissionsExt;
        let p = d.join("greentopo");
        std::fs::write(
            &p,
            format!(
                "#!/bin/sh\n[ \"$*\" = '--probe' ] && exit 0\n\
                 printf '%s\\n' \"$*\" >> \"{calls}\"\n\
                 case \"$*\" in\n\
                   'hook install --check') exit {check_rc} ;;\n\
                   'hook install') printf '  wrote  {script}  (regenerated)\\n  wrote  {settings}  (merged)\\n'; exit 0 ;;\n\
                   *) echo 'usage' >&2; exit 2 ;;\n\
                 esac\n",
                calls = d.join("calls").display(),
                script = SCRIPT_PATH,
                settings = SETTINGS_PATH,
            ),
        )
        .unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        // ETXTBSY on Linux: a sibling test thread can `fork` while the write
        // above still has the file open, and the child holds that descriptor
        // until its own `exec` — during which the kernel refuses to execute a
        // file open for writing (rust-lang/rust#114554). CI hit it once in
        // ~160 parallel tests. Probe until the window closes; the probe's own
        // `--probe` argument exits before the script records the call, so the
        // tests that assert on what greentopo was asked see nothing of it.
        for _ in 0..200 {
            match Command::new(&p).arg("--probe").output() {
                Err(e) if e.raw_os_error() == Some(26) => {
                    std::thread::sleep(std::time::Duration::from_millis(5))
                }
                _ => break,
            }
        }
        p.display().to_string()
    }

    #[cfg(unix)]
    fn calls(d: &Path) -> String {
        std::fs::read_to_string(d.join("calls")).unwrap_or_default()
    }

    #[cfg(unix)]
    fn git_repo(d: &Path) {
        assert!(Command::new("git")
            .args(["init", "-q"])
            .current_dir(d)
            .status()
            .unwrap()
            .success());
    }

    #[cfg(unix)]
    #[test]
    fn a_repo_without_a_registry_is_not_asked() {
        let d = dir("no-corpus");
        let greentopo = fake_greentopo(&d, 1);
        let repo = d.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        assert_eq!(state(&repo, &greentopo), GreentopoHook::default());
        assert_eq!(calls(&d), "", "no spawn for a repository with no corpus");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[cfg(unix)]
    #[test]
    fn greentopos_exit_code_is_the_state() {
        for (rc, want) in [
            (0, GreentopoHookState::Current),
            (1, GreentopoHookState::Stale),
        ] {
            let d = dir(&format!("rc{rc}"));
            let greentopo = fake_greentopo(&d, rc);
            let repo = d.join("repo");
            std::fs::create_dir_all(&repo).unwrap();
            std::fs::write(repo.join(REGISTRY), "dir: docs/adr\n").unwrap();
            let got = state(&repo, &greentopo);
            assert_eq!(got.state, want);
            assert!(calls(&d).contains("hook install --check"));
            let _ = std::fs::remove_dir_all(&d);
        }
    }

    #[cfg(unix)]
    #[test]
    fn an_exit_code_greentopo_did_not_promise_is_unknown_not_a_state() {
        let d = dir("rc2");
        let greentopo = fake_greentopo(&d, 2);
        let repo = d.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(repo.join(REGISTRY), "dir: docs/adr\n").unwrap();
        let got = state(&repo, &greentopo);
        assert!(
            matches!(got.state, GreentopoHookState::Unknown { ref why } if why.contains("exited 2")),
            "{got:?}"
        );
        assert_ne!(
            got.state,
            GreentopoHookState::Stale,
            "unknown must never plan an install"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_missing_binary_is_reported_not_read_as_current() {
        let d = dir("no-greentopo");
        let repo = d.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(repo.join(REGISTRY), "dir: docs/adr\n").unwrap();
        let got = state(
            &repo,
            &d.join("definitely-not-greentopo").display().to_string(),
        );
        assert_eq!(got.state, GreentopoHookState::NoGreentopo);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[cfg(unix)]
    #[test]
    fn a_gitignored_hook_is_named() {
        let d = dir("ignored");
        let greentopo = fake_greentopo(&d, 0);
        let repo = d.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git_repo(&repo);
        std::fs::write(repo.join(REGISTRY), "dir: docs/adr\n").unwrap();
        std::fs::write(repo.join(".gitignore"), ".claude/\n").unwrap();
        let got = state(&repo, &greentopo);
        assert_eq!(got.state, GreentopoHookState::Current);
        assert_eq!(
            got.ignored,
            vec![SCRIPT_PATH.to_string(), SETTINGS_PATH.to_string()]
        );

        // The negation shape every wired repository uses clears it.
        std::fs::write(
            repo.join(".gitignore"),
            ".claude/*\n!.claude/settings.json\n!.claude/hooks/\n.claude/hooks/*\n!.claude/hooks/greentopo-heads.sh\n",
        )
        .unwrap();
        assert!(state(&repo, &greentopo).ignored.is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[cfg(unix)]
    #[test]
    fn install_counts_what_greentopo_says_it_wrote() {
        let d = dir("install");
        let greentopo = fake_greentopo(&d, 1);
        let repo = d.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        assert_eq!(install(&repo, &greentopo), Ok(2));
        assert!(calls(&d).lines().any(|l| l == "hook install"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_failed_install_carries_greentopos_words() {
        let d = dir("install-fail");
        let repo = d.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let err = install(&repo, &d.join("nope").display().to_string()).unwrap_err();
        assert!(err.contains("nope"), "{err}");
        let _ = std::fs::remove_dir_all(&d);
    }
}
