//! `greengate land --detach` and `--wait` against a real bare remote: the
//! launch, the pinned sha, the lock, the dead worker and the signals. A
//! worker is held at a known point by a `git` shim that parks `ls-remote`
//! until a `go` file exists, so no test races the worker with a sleep.

use crate::common;
use crate::land::{field, fixture, head, hook, land, plain_git, receipt_field, tip, Fx, Ran};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn run(fx: &Fx, args: &[&str], path_prefix: Option<&Path>) -> Ran {
    let mut cmd = common::amont_cmd();
    cmd.arg("land")
        .args(args)
        .current_dir(&fx.r.dir)
        .stdin(Stdio::null());
    common::Repo::strip_git_env_impl(&mut cmd);
    if let Some(prefix) = path_prefix {
        let rest = std::env::var_os("PATH").unwrap_or_default();
        let joined = std::env::join_paths(
            std::iter::once(prefix.to_path_buf()).chain(std::env::split_paths(&rest)),
        )
        .expect("PATH");
        cmd.env("PATH", joined);
    }
    let out = cmd.output().expect("run greengate land");
    Ran {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// A `git` that parks every `ls-remote` until `<dir>/go` exists.
fn parking_shim(fx: &Fx) -> PathBuf {
    let real = String::from_utf8(
        Command::new("sh")
            .args(["-c", "command -v git"])
            .output()
            .expect("sh")
            .stdout,
    )
    .expect("utf8");
    let dir = fx.r.path(".git/parkshim");
    std::fs::create_dir_all(&dir).expect("mkdir");
    common::fixture_exec(
        &dir.join("git"),
        &format!(
            "#!/bin/sh\nfor a in \"$@\"; do\n  if [ \"$a\" = ls-remote ]; then\n    while [ ! -e '{}/go' ]; do sleep 0.05; done\n  fi\ndone\nexec {} \"$@\"\n",
            dir.display(),
            real.trim()
        ),
    );
    dir
}

fn release(shim: &Path) {
    std::fs::write(shim.join("go"), "").expect("go");
}

fn receipt_of(launched: &Ran) -> String {
    field(launched, "receipt")
}

fn wait_for(fx: &Fx, receipt: &str) -> Ran {
    run(fx, &["--wait", receipt], None)
}

/// The worker's pid, once it has taken over the receipt.
fn worker_pid(receipt: &str) -> u32 {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let body = std::fs::read_to_string(receipt).unwrap_or_default();
        if let Some(i) = body.find("\"worker_pid\":") {
            let digits: String = body[i + "\"worker_pid\":".len()..]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            if let Ok(pid) = digits.parse() {
                return pid;
            }
        }
        assert!(Instant::now() < deadline, "no worker_pid in {body}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn signal(pid: u32, name: &str) {
    let status = Command::new("kill")
        .arg(name)
        .arg(pid.to_string())
        .status()
        .expect("kill");
    assert!(status.success(), "kill {name} {pid}");
}

/// A landed base, then one new commit: the branch to detach.
fn ahead() -> Fx {
    let fx = fixture("feat");
    assert_eq!(land(&fx, &[]).code, 0);
    fx.r.stage("c.txt", "more\n");
    fx.r.commit("more");
    fx
}

fn receipt_text(path: &str) -> String {
    std::fs::read_to_string(path).expect("receipt")
}

#[test]
fn a_detached_land_returns_at_once_and_finishes_landed() {
    let fx = ahead();
    let judged = head(&fx);
    let started = Instant::now();
    let launched = run(&fx, &["--detach", "--json"], None);
    assert_eq!(launched.code, 0, "{}", launched.stderr);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "returns in seconds"
    );
    assert_eq!(field(&launched, "status"), "detached");
    let receipt = receipt_of(&launched);
    assert!(
        field(&launched, "wait").starts_with("greengate land --wait '"),
        "{}",
        launched.stdout
    );
    assert!(worker_pid(&receipt) > 0);

    let waited = wait_for(&fx, &receipt);
    assert_eq!(waited.code, 0, "{}", waited.stderr);
    assert!(waited.stdout.starts_with("landed "), "{}", waited.stdout);
    assert!(
        waited
            .stderr
            .contains(&format!("waiting on {receipt} (log: ")),
        "{}",
        waited.stderr
    );
    let body = receipt_text(&receipt);
    assert_eq!(receipt_field(&body, "status"), "landed");
    assert_eq!(receipt_field(&body, "pushed"), judged);
    assert_eq!(receipt_field(&body, "judged"), judged);
    assert_eq!(tip(&fx, "feat"), judged);
    let remote_head = plain_git(&fx.r.dir, &["ls-remote", "origin", "refs/heads/feat"]);
    assert!(remote_head.starts_with(&judged), "{remote_head}");
    assert_eq!(receipt_field(&body, "remote_after"), judged);
    assert!(body.contains("\"gates\":{"), "{body}");
    assert!(body.contains("\"exit\":0"), "{body}");
}

#[test]
fn a_launch_without_json_prints_the_wait_command_alone() {
    let fx = ahead();
    let launched = run(&fx, &["--detach"], None);
    assert_eq!(launched.code, 0, "{}", launched.stderr);
    assert!(
        launched
            .stdout
            .trim()
            .starts_with("greengate land --wait '"),
        "{}",
        launched.stdout
    );
    assert_eq!(launched.stdout.trim().lines().count(), 1);
    assert!(launched.stderr.contains("detached "), "{}", launched.stderr);
    assert!(
        launched
            .stderr
            .contains("not landed yet: wait on the receipt"),
        "{}",
        launched.stderr
    );
    let receipt = launched
        .stdout
        .trim()
        .trim_start_matches("greengate land --wait '")
        .trim_end_matches('\'')
        .to_string();
    assert_eq!(wait_for(&fx, &receipt).code, 0);
}

#[test]
fn a_commit_made_after_the_launch_is_never_pushed() {
    let fx = ahead();
    let before = tip(&fx, "feat");
    let shim = parking_shim(&fx);
    let launched = run(&fx, &["--detach", "--json"], Some(&shim));
    assert_eq!(launched.code, 0, "{}", launched.stderr);
    fx.r.stage("d.txt", "late\n");
    fx.r.commit("late");
    release(&shim);
    let waited = wait_for(&fx, &receipt_of(&launched));
    assert_eq!(waited.code, 1, "{}{}", waited.stdout, waited.stderr);
    let body = receipt_text(&receipt_of(&launched));
    assert_eq!(receipt_field(&body, "status"), "refused");
    assert_eq!(tip(&fx, "feat"), before, "nothing was pushed");
}

#[test]
fn a_hook_that_moves_head_is_refused_and_never_retried() {
    let fx = fixture("feat");
    assert_eq!(land(&fx, &[]).code, 0);
    let other = fx.r.path(".git/other");
    plain_git(
        fx.r.dir.as_path(),
        &[
            "clone",
            "-q",
            fx.bare.to_str().expect("utf8"),
            other.to_str().expect("utf8"),
        ],
    );
    plain_git(&other, &["config", "user.email", "o@example.com"]);
    plain_git(&other, &["config", "user.name", "o"]);
    plain_git(&other, &["switch", "-q", "feat"]);
    plain_git(&other, &["commit", "-q", "--allow-empty", "-m", "ahead"]);
    plain_git(&other, &["push", "-q", "origin", "feat"]);
    plain_git(&fx.r.dir, &["fetch", "-q", "origin"]);
    let before = tip(&fx, "feat");
    hook(
        &fx,
        "if [ \"$(git rev-parse HEAD)\" != \"$(git rev-parse origin/feat)\" ]; then\n  git merge -q --ff-only origin/feat\n  echo 'Rebased onto origin/feat. This push refs predate the rebase - push again.'\n  exit 1\nfi",
    );
    let launched = run(&fx, &["--detach", "--json"], None);
    assert_eq!(launched.code, 0, "{}", launched.stderr);
    let waited = wait_for(&fx, &receipt_of(&launched));
    assert_eq!(waited.code, 1, "{}{}", waited.stdout, waited.stderr);
    let body = receipt_text(&receipt_of(&launched));
    assert_eq!(receipt_field(&body, "status"), "refused");
    assert!(body.contains("\"retried\":false"), "{body}");
    // Refused because the hook moved HEAD, not on some earlier path.
    let log = std::fs::read_to_string(receipt_field(&body, "log")).expect("log");
    assert!(log.contains("pre-push hook moved HEAD"), "{log}");
    assert_eq!(tip(&fx, "feat"), before);
}

#[test]
fn a_second_launch_while_a_worker_lives_is_refused_and_names_the_receipt() {
    let fx = ahead();
    let shim = parking_shim(&fx);
    let first = run(&fx, &["--detach", "--json"], Some(&shim));
    assert_eq!(first.code, 0, "{}", first.stderr);
    let receipt = receipt_of(&first);
    let second = run(&fx, &["--detach", "--json"], Some(&shim));
    assert_eq!(second.code, 1, "{}", second.stdout);
    assert!(second.stderr.contains(&receipt), "{}", second.stderr);
    assert!(second.stderr.contains("--wait"), "{}", second.stderr);
    release(&shim);
    assert_eq!(wait_for(&fx, &receipt).code, 0);
    // Two lands in a row: the second one is accepted once the first ended.
    fx.r.stage("e.txt", "again\n");
    fx.r.commit("again");
    let third = run(&fx, &["--detach", "--json"], None);
    assert_eq!(third.code, 0, "{}", third.stderr);
    assert_eq!(wait_for(&fx, &receipt_of(&third)).code, 0);
    let logs: Vec<_> = std::fs::read_dir(fx.r.path(".git/greengate/land"))
        .expect("dir")
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "log"))
        .collect();
    assert_eq!(logs.len(), 2, "one log per land");
    for log in logs {
        assert!(
            std::fs::metadata(log.path()).expect("meta").len() > 0,
            "{} is intact",
            log.path().display()
        );
    }
}

#[test]
fn an_immediate_wait_after_each_launch_never_reports_a_death() {
    let fx = ahead();
    for round in 0..5 {
        let launched = run(&fx, &["--detach", "--json"], None);
        assert_eq!(launched.code, 0, "round {round}: {}", launched.stderr);
        let waited = wait_for(&fx, &receipt_of(&launched));
        assert!(
            !waited.stdout.contains("died"),
            "round {round}: {}",
            waited.stdout
        );
        assert_eq!(
            waited.code, 0,
            "round {round}: {}{}",
            waited.stdout, waited.stderr
        );
        fx.r.stage(&format!("r{round}.txt"), "x\n");
        fx.r.commit(&format!("round {round}"));
    }
}

#[test]
fn a_killed_worker_is_dead_within_a_poll_and_leaks_no_lock() {
    let fx = ahead();
    let pids = fx.r.path(".git/pids");
    hook(
        &fx,
        &format!(
            "echo $$ >> '{0}'\nsleep 60 &\necho $! >> '{0}'\nwait",
            pids.display()
        ),
    );
    let launched = run(&fx, &["--detach", "--json"], None);
    assert_eq!(launched.code, 0, "{}", launched.stderr);
    let receipt = receipt_of(&launched);
    let worker = worker_pid(&receipt);
    let deadline = Instant::now() + Duration::from_secs(30);
    while !pids.exists()
        || std::fs::read_to_string(&pids)
            .unwrap_or_default()
            .lines()
            .count()
            < 2
    {
        assert!(Instant::now() < deadline, "the hook never ran");
        std::thread::sleep(Duration::from_millis(50));
    }
    signal(worker, "-9");
    let started = Instant::now();
    let waited = wait_for(&fx, &receipt);
    assert!(
        started.elapsed() < Duration::from_secs(6),
        "within one poll"
    );
    assert_eq!(waited.code, 1, "{}{}", waited.stdout, waited.stderr);
    assert!(
        waited.stdout.contains(&format!("worker {worker} died")),
        "{}",
        waited.stdout
    );
    assert!(waited.stdout.contains("log: '"), "{}", waited.stdout);
    assert!(
        waited.stdout.contains("rerun: greengate land --detach"),
        "{}",
        waited.stdout
    );
    // The hook and its sleep outlive the worker; neither holds the lock.
    hook(&fx, "exit 0");
    let next = run(&fx, &["--detach", "--json"], None);
    for pid in std::fs::read_to_string(&pids).unwrap_or_default().lines() {
        let _ = Command::new("kill").args(["-9", pid]).status(); // cleanup: the pid may already be gone
    }
    assert_eq!(next.code, 0, "{}", next.stderr);
    assert_eq!(wait_for(&fx, &receipt_of(&next)).code, 0);
}

#[test]
fn an_early_refusal_leaves_no_receipt_running() {
    let fx = ahead();
    plain_git(&fx.r.dir, &["checkout", "-q", "--detach"]);
    for round in 0..5 {
        let launched = run(&fx, &["--detach", "--json"], None);
        assert_eq!(launched.code, 0, "round {round}: {}", launched.stderr);
        let waited = wait_for(&fx, &receipt_of(&launched));
        assert_eq!(
            waited.code, 1,
            "round {round}: {}{}",
            waited.stdout, waited.stderr
        );
        assert!(waited.stdout.starts_with("refused"), "{}", waited.stdout);
    }
    for body in crate::land::receipts(&fx) {
        assert_ne!(receipt_field(&body, "status"), "running", "{body}");
    }
}

#[test]
fn sigterm_finalizes_the_receipt_interrupted() {
    let fx = ahead();
    let shim = parking_shim(&fx);
    let launched = run(&fx, &["--detach", "--json"], Some(&shim));
    assert_eq!(launched.code, 0, "{}", launched.stderr);
    let receipt = receipt_of(&launched);
    signal(worker_pid(&receipt), "-TERM");
    let waited = wait_for(&fx, &receipt);
    release(&shim);
    assert_eq!(waited.code, 143, "{}{}", waited.stdout, waited.stderr);
    assert!(
        waited.stdout.starts_with("interrupted"),
        "{}",
        waited.stdout
    );
    assert_eq!(
        receipt_field(&receipt_text(&receipt), "status"),
        "interrupted"
    );
}

#[test]
fn a_dry_run_cannot_be_detached_and_wait_stands_alone() {
    let fx = ahead();
    assert_eq!(run(&fx, &["--detach", "--dry-run"], None).code, 2);
    assert_eq!(run(&fx, &["--wait", "x", "--force"], None).code, 2);
    assert_eq!(run(&fx, &["--detach", "--wait", "x"], None).code, 2);
    let missing = run(&fx, &["--wait", "/nonexistent/receipt.json"], None);
    assert_eq!(missing.code, 2, "{}", missing.stderr);
}

#[test]
fn an_annotated_tag_lands_detached() {
    let fx = ahead();
    plain_git(&fx.r.dir, &["tag", "-a", "v9", "-m", "release v9"]);
    let launched = run(&fx, &["--detach", "--json", "--tag", "v9"], None);
    assert_eq!(launched.code, 0, "{}", launched.stderr);
    let waited = wait_for(&fx, &receipt_of(&launched));
    assert_eq!(waited.code, 0, "{}{}", waited.stdout, waited.stderr);
    let body = receipt_text(&receipt_of(&launched));
    assert_eq!(receipt_field(&body, "status"), "landed");
    assert_eq!(
        receipt_field(&body, "pushed"),
        receipt_field(&body, "judged")
    );
}

#[test]
fn a_detached_tag_is_never_forced() {
    let fx = ahead();
    let ran = run(&fx, &["--detach", "--tag", "v1", "--force"], None);
    assert_eq!(ran.code, 2, "{}", ran.stderr);
}

#[test]
fn a_refused_launch_under_json_still_prints_json() {
    let fx = ahead();
    let shim = parking_shim(&fx);
    let first = run(&fx, &["--detach", "--json"], Some(&shim));
    assert_eq!(first.code, 0, "{}", first.stderr);
    let second = run(&fx, &["--detach", "--json"], Some(&shim));
    assert_eq!(second.code, 1, "{}", second.stderr);
    assert!(
        second.stdout.trim_start().starts_with('{'),
        "{}",
        second.stdout
    );
    assert!(
        second.stdout.contains("\"status\":\"refused\""),
        "{}",
        second.stdout
    );
    release(&shim);
    assert_eq!(wait_for(&fx, &receipt_of(&first)).code, 0);
}

#[test]
fn the_foreground_json_keeps_its_keys() {
    let fx = ahead();
    let ran = run(&fx, &["--json"], None);
    assert_eq!(ran.code, 0, "{}", ran.stderr);
    let keys = [
        "branch",
        "remote",
        "push_url",
        "pushed",
        "ref",
        "remote_before",
        "remote_after",
        "retried",
        "status",
        "gates",
    ];
    let mut at = 0;
    for key in keys {
        let found = ran.stdout[at..]
            .find(&format!("\"{key}\":"))
            .unwrap_or_else(|| panic!("{key} missing or out of order in {}", ran.stdout));
        at += found;
    }
    assert!(!ran.stdout.contains("\"judged\""), "{}", ran.stdout);
}
