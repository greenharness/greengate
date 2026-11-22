//! The greengate the pre-push hook runs, against the one running `land`.
//!
//! A built-in gate's stamp is sealed under the version of the binary that ran
//! it ([`crate::gate_identity::builtin_token`]). Land's pre-stage runs in land's
//! own binary; the push it then makes runs the repository's pre-push hook,
//! which runs the binary baked into the hook. When the two versions differ,
//! the pre-stage finds the tree stamped and the hook finds no stamp of its own
//! version: `evidence-missing`, and `rehearse --wait` says there is nothing to
//! rehearse, so following the refusal's advice loops (follow-up f06, seen
//! 2026-11-21 when a worktree's `pnpm install` re-baked the shared hooks to
//! its `node_modules` greengate).
//!
//! So land asks first, before the pre-stage: which binary would the hook run,
//! and what version is it? A different version refuses at once, naming both.
//! Every question this cannot answer (a hook that is not ours, a binary that
//! does not answer `--version`) lets land go on as before: a probe that fails
//! is never a new way to refuse.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// The pre-push hook would run another greengate version than this one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Split {
    /// The binary the hook resolves to, as the shim would spell it.
    pub hook_bin: String,
    pub hook_version: String,
}

impl Split {
    /// The refusal's one line.
    pub fn why(&self) -> String {
        format!(
            "the pre-push hook runs greengate {} ({}), this land is greengate {}: \
             the stamps land makes would not count at that hook; nothing was pushed",
            self.hook_version,
            self.hook_bin,
            env!("CARGO_PKG_VERSION"),
        )
    }

    /// What to run instead: re-bake the hooks to this binary, or land with
    /// the hook's. A re-bake does not stick in a repository whose `prepare`
    /// runs `greengate init` from its own `node_modules`, hence the second.
    pub fn next(&self, me: &Path, again: &str) -> Vec<String> {
        let land_args = again.split_once(' ').map_or("land", |(_, rest)| rest);
        vec![
            format!("{} init", me.display()),
            again.to_string(),
            format!("{} {land_args}", self.hook_bin),
        ]
    }
}

/// The split, when there is one: this repository's pre-push hook is our shim
/// and resolves to a binary whose version differs from this one's.
pub(crate) fn split() -> Option<Split> {
    let installed = crate::install::installed_here();
    // Not our shim (a hand-written hook, none at all): no greengate runs at
    // the push, so there is nothing to disagree with.
    let (_, baked) = installed
        .shims
        .iter()
        .find(|(name, _)| *name == "pre-push")?;
    let hook_bin = resolve(
        std::env::var_os("GIT_HOOKS_BIN").as_deref(),
        baked.as_deref(),
        std::env::var_os("HOME").as_deref(),
        &|tool| crate::hooks::common::which(tool),
    )?;
    let me = std::env::current_exe().ok()?;
    if same_file(Path::new(&hook_bin), &me) {
        return None;
    }
    let hook_version = version_of(&hook_bin)?;
    (hook_version != env!("CARGO_PKG_VERSION")).then_some(Split {
        hook_bin,
        hook_version,
    })
}

/// The binary the hook shim would exec, in the shim's own order
/// (`templates/hooks/pre-commit`, the `BIN=` chain): `$GIT_HOOKS_BIN`, the
/// baked path, `~/.local/bin`, then `PATH`. Keep the two in step.
pub(crate) fn resolve(
    git_hooks_bin: Option<&OsStr>,
    baked: Option<&str>,
    home: Option<&OsStr>,
    which: &dyn Fn(&str) -> Option<String>,
) -> Option<String> {
    let runnable = |p: &Path| crate::gate_identity::is_executable(p);
    if let Some(bin) = git_hooks_bin.filter(|b| !b.is_empty()) {
        if runnable(Path::new(bin)) {
            return Some(bin.to_string_lossy().into_owned());
        }
    }
    if let Some(bin) = baked.filter(|b| is_absolute(b)) {
        if runnable(Path::new(bin)) {
            return Some(bin.to_string());
        }
    }
    if let Some(home) = home.filter(|h| !h.is_empty()) {
        let local = PathBuf::from(home).join(".local").join("bin");
        for name in ["greengate", "greengate.exe", "amont", "amont.exe"] {
            let p = local.join(name);
            if runnable(&p) {
                return Some(p.to_string_lossy().into_owned());
            }
        }
    }
    which("greengate").or_else(|| which("amont"))
}

/// The shim's own test: POSIX `/…` or a Windows drive path.
fn is_absolute(p: &str) -> bool {
    let b = p.as_bytes();
    p.starts_with('/')
        || (b.len() >= 3
            && b[0].is_ascii_alphabetic()
            && b[1] == b':'
            && matches!(b[2], b'/' | b'\\'))
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// `<bin> --version`, bounded; `greengate 4.12.0` (or `amont 2.3.1`) → the
/// version. Anything else is no answer.
fn version_of(bin: &str) -> Option<String> {
    static NEVER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let deadline = std::time::Instant::now() + crate::hooks::common::PROBE_DEADLINE;
    let argv = [bin.to_string(), "--version".to_string()];
    let cwd = std::env::current_dir().ok()?;
    let out = crate::tree_run::run_output(&argv, &cwd, deadline, &NEVER).ok()?;
    parse_version(&out)
}

pub(crate) fn parse_version(out: &str) -> Option<String> {
    let mut words = out.lines().next()?.split_whitespace();
    let (name, version) = (words.next()?, words.next()?);
    let ours = matches!(name, "greengate" | "amont");
    let shaped = version.starts_with(|c: char| c.is_ascii_digit()) && version.contains('.');
    (ours && shaped).then(|| version.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("greengate-land-hook-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir); // best-effort: a scratch path
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[cfg(unix)]
    fn exec(path: &Path) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn land_hook_resolves_in_the_shims_order() {
        let dir = scratch("order");
        let env_bin = dir.join("env/greengate");
        let baked = dir.join("baked/greengate");
        let home = dir.join("home");
        let local = home.join(".local/bin/greengate");
        let none = |_: &str| None;
        let path = |t: &str| (t == "greengate").then(|| "greengate".to_string());
        let s = |p: &Path| p.to_str().unwrap().to_string();

        // Nothing exists yet: only PATH answers.
        let r = |e: Option<&Path>, b: Option<&Path>, w: &dyn Fn(&str) -> Option<String>| {
            resolve(
                e.map(Path::as_os_str),
                b.and_then(Path::to_str),
                Some(home.as_os_str()),
                w,
            )
        };
        assert_eq!(
            r(Some(&env_bin), Some(&baked), &path),
            Some("greengate".into())
        );
        assert_eq!(r(Some(&env_bin), Some(&baked), &none), None);
        exec(&local);
        assert_eq!(r(Some(&env_bin), Some(&baked), &path), Some(s(&local)));
        exec(&baked);
        assert_eq!(r(Some(&env_bin), Some(&baked), &path), Some(s(&baked)));
        exec(&env_bin);
        assert_eq!(r(Some(&env_bin), Some(&baked), &path), Some(s(&env_bin)));
        // A relative baked path is the shim's "unbaked": skipped.
        assert_eq!(
            resolve(None, Some("greengate"), Some(OsStr::new("")), &none),
            None
        );
        let _ = std::fs::remove_dir_all(&dir); // best-effort: a scratch path
    }

    #[test]
    fn land_hook_reads_only_our_version_line() {
        assert_eq!(parse_version("greengate 4.12.0\n"), Some("4.12.0".into()));
        assert_eq!(parse_version("amont 2.3.1"), Some("2.3.1".into()));
        assert_eq!(parse_version("git version 2.47.0"), None);
        assert_eq!(parse_version("greengate: unknown flag"), None);
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn land_hook_names_both_versions_and_both_ways_out() {
        let split = Split {
            hook_bin: "/repo/node_modules/greengate".into(),
            hook_version: "4.11.0".into(),
        };
        let why = split.why();
        assert!(
            why.contains("greengate 4.11.0 (/repo/node_modules/greengate)"),
            "{why}"
        );
        assert!(why.contains(&format!(
            "this land is greengate {}",
            env!("CARGO_PKG_VERSION")
        )));
        let next = split.next(
            Path::new("/usr/local/bin/greengate"),
            "greengate land --tag v1",
        );
        assert_eq!(
            next,
            [
                "/usr/local/bin/greengate init",
                "greengate land --tag v1",
                "/repo/node_modules/greengate land --tag v1",
            ]
        );
    }
}
