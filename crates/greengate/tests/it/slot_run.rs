//! `amont slot run` and `amont slots` (ADR-0014): any command queues for a
//! host slot, and the queue says who holds it.
//!
//! Unpinned, like `busy_defers.rs`: `.cargo/config.toml` turns slots off for
//! the test process. Each test turns them on again, removes any held marker,
//! and uses a slot directory of its own with one fixed slot, so no load test
//! or ramp is involved.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn slot_dir(tag: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "greengate-slotrun-{}-{tag}-{seq}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn amont(slots: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_greengate"));
    crate::common::sandbox_env(&mut cmd);
    cmd.args(args)
        .current_dir(crate::common::sandbox().join("cwd"))
        .env_remove("GREENHARNESS_HOST_SLOT")
        .env("GREENHARNESS_SLOT_DIR", slots)
        .env("GREENGATE_HOST_SLOTS", "1")
        // Pinned so every width and cap here is the same on any host.
        .env("GREENGATE_HOST_CORES", cores().to_string());
    cmd
}

fn spawn(slots: &Path, args: &[&str]) -> Child {
    amont(slots, args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn amont")
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// `slots --json` carries the core count and a stable token for its source.
#[test]
fn slots_json_names_the_cores_and_their_source() {
    let slots = slot_dir("json-cores");
    let out = amont(&slots, &["slots", "--json"])
        .output()
        .expect("amont slots --json");
    let json = String::from_utf8_lossy(&out.stdout);
    assert!(
        json.contains("\"cores\":8,\"cores_source\":\"env\""),
        "{json}"
    );
    let _ = std::fs::remove_dir_all(&slots); // best-effort: a scratch dir
}

/// Wait until `amont slots --json` shows `waiters` waiters, so the next one queues
/// behind them in ticket order, never by a guessed sleep.
fn until_waiting(slots: &Path, waiters: usize) {
    let deadline = Instant::now() + Duration::from_secs(30);
    let want = format!("\"waiting\":{waiters}");
    loop {
        let out = amont(slots, &["slots", "--json"])
            .output()
            .expect("amont slots");
        let json = String::from_utf8_lossy(&out.stdout).into_owned();
        if json.contains(&want) {
            return;
        }
        assert!(Instant::now() < deadline, "no {waiters} waiting: {json}");
        std::thread::yield_now();
    }
}

/// Wait until `amont slots --json` shows a holder, so a test never races the
/// first wrapper's admission with a sleep.
fn until_held(slots: &Path) -> String {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let out = amont(slots, &["slots", "--json"])
            .output()
            .expect("amont slots");
        let json = String::from_utf8_lossy(&out.stdout).into_owned();
        if json.contains("\"held\":1") {
            return json;
        }
        assert!(Instant::now() < deadline, "no holder appeared: {json}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// One slot, two wrapped commands: the second waits, says whom it waits
/// behind, and runs after the first; each exit status passes through.
#[test]
fn a_second_command_queues_behind_the_first_and_names_it() {
    let slots = slot_dir("queue");
    let first = spawn(
        &slots,
        &["slot", "run", "--", "sh", "-c", "sleep 3; exit 3"],
    );
    let json = until_held(&slots);
    assert!(json.contains("\"tool\":\"greengate slot run\""), "{json}");
    assert!(json.contains("sleep 3"), "{json}");
    let second = amont(
        &slots,
        &["slot", "run", "--", "sh", "-c", "echo second-ran"],
    )
    .output()
    .expect("second");
    let out = text(&second);
    assert_eq!(second.status.code(), Some(0), "{out}");
    assert!(
        out.contains("queued behind greengate slot run: sh -c sleep 3"),
        "{out}"
    );
    assert!(out.contains("second-ran"), "{out}");
    let first = first.wait_with_output().expect("first");
    assert_eq!(first.status.code(), Some(3), "{}", text(&first));
    let waits = std::fs::read_to_string(slots.join("waits.jsonl")).unwrap_or_default();
    assert_eq!(
        waits.matches("\"outcome\":\"admitted\"").count(),
        2,
        "{waits}"
    );
    let _ = std::fs::remove_dir_all(&slots); // best-effort: scratch
}

/// A wait past `--max-wait` exits 75 and runs nothing (ADR-0022), and its
/// journal row says the ceiling ended it.
#[test]
fn a_timed_out_wait_exits_75_without_running_anything() {
    let slots = slot_dir("timeout");
    let mut holder = spawn(&slots, &["slot", "run", "--", "sleep", "4"]);
    until_held(&slots);
    let marker = slots.join("ran");
    let out = amont(
        &slots,
        &[
            "slot",
            "run",
            "--max-wait",
            "1",
            "--",
            "touch",
            marker.to_str().unwrap(),
        ],
    )
    .output()
    .expect("timed out");
    assert_eq!(out.status.code(), Some(75), "{}", text(&out));
    assert!(!marker.exists(), "nothing may run on a timeout");
    assert!(
        text(&out).contains("no host slot within --max-wait 1s"),
        "{}",
        text(&out)
    );
    assert!(
        text(&out).contains("pass --now to skip the queue"),
        "{}",
        text(&out)
    );
    let waits = std::fs::read_to_string(slots.join("waits.jsonl")).unwrap_or_default();
    assert!(
        waits.contains("\"outcome\":\"timeout\",\"end\":\"ceiling\""),
        "{waits}"
    );
    let _ = holder.wait(); // best-effort: reap the holder
    let _ = std::fs::remove_dir_all(&slots); // best-effort: scratch
}

/// A line with no progress for the stall window exits 75 before
/// `--max-wait`, says so, and journals `"end":"stalled"` (ADR-0022).
#[test]
fn a_stalled_line_exits_75_before_the_max_wait() {
    let slots = slot_dir("stall");
    let mut holder = spawn(&slots, &["slot", "run", "--", "sleep", "8"]);
    until_held(&slots);
    let marker = slots.join("ran");
    let started = Instant::now();
    let out = amont(
        &slots,
        &[
            "slot",
            "run",
            "--max-wait",
            "30",
            "--",
            "touch",
            marker.to_str().unwrap(),
        ],
    )
    .env("GREENHARNESS_SLOT_STALL_S", "2")
    .output()
    .expect("stalled");
    assert_eq!(out.status.code(), Some(75), "{}", text(&out));
    assert!(
        started.elapsed() < Duration::from_secs(7),
        "it waited for the holder"
    );
    assert!(!marker.exists(), "nothing may run on a stall");
    assert!(
        text(&out).contains("no progress in the host slot line for 2s"),
        "{}",
        text(&out)
    );
    let waits = std::fs::read_to_string(slots.join("waits.jsonl")).unwrap_or_default();
    assert!(waits.contains("\"end\":\"stalled\""), "{waits}");
    let _ = holder.kill(); // best-effort: the holder is not under test
    let _ = holder.wait(); // best-effort: reap the killed holder
    let _ = std::fs::remove_dir_all(&slots); // best-effort: scratch
}

/// A line that moves never stalls: with three short holders ahead, each
/// admission comes sooner than the stall window, so the last waiter is
/// admitted though its whole wait is longer than the window.
#[test]
fn a_moving_line_outlasts_the_stall_window() {
    let slots = slot_dir("moving");
    let mut ahead = vec![spawn(&slots, &["slot", "run", "--", "sleep", "2"])];
    until_held(&slots);
    for _ in 0..2 {
        let mut cmd = amont(
            &slots,
            &["slot", "run", "--max-wait", "60", "--", "sleep", "2"],
        );
        cmd.env("GREENHARNESS_SLOT_STALL_S", "4")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        ahead.push(cmd.spawn().expect("a waiter ahead"));
        // Ticket order: the next waiter joins only once this one is queued.
        until_waiting(&slots, ahead.len() - 1);
    }
    let started = Instant::now();
    let out = amont(&slots, &["slot", "run", "--max-wait", "60", "--", "true"])
        .env("GREENHARNESS_SLOT_STALL_S", "4")
        .output()
        .expect("the last waiter");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(
        started.elapsed() > Duration::from_secs(4),
        "the wait did not outlast the window, so it proves nothing: {:?}",
        started.elapsed()
    );
    for child in ahead {
        let _ = child.wait_with_output(); // best-effort: reap
    }
    let _ = std::fs::remove_dir_all(&slots); // best-effort: scratch
}

/// `--on-timeout run` is refused (ADR-0022): a wait that ends runs nothing.
/// `--now` is the up-front way to skip the queue, and it takes no value.
#[test]
fn on_timeout_run_is_refused_and_now_is_a_switch() {
    let slots = slot_dir("flags");
    let out = amont(
        &slots,
        &["slot", "run", "--on-timeout", "run", "--", "true"],
    )
    .output()
    .expect("refused");
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(
        text(&out).contains("--on-timeout was removed"),
        "{}",
        text(&out)
    );
    assert!(
        text(&out).contains("slot run --now -- <cmd>"),
        "{}",
        text(&out)
    );
    for line in text(&out).lines() {
        assert!(line.chars().count() < 80, "over 79 columns: {line}");
    }
    for bad in [&["--now=1"][..], &["--now", "--max-wait", "5"][..]] {
        let mut args = vec!["slot", "run"];
        args.extend_from_slice(bad);
        args.extend_from_slice(&["--", "true"]);
        let out = amont(&slots, &args).output().expect("usage");
        assert_eq!(out.status.code(), Some(2), "{bad:?}: {}", text(&out));
    }
    let out = amont(
        &slots,
        &["slot", "run", "--on-timeout", "exit", "--", "true"],
    )
    .output()
    .expect("deprecated");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(text(&out).contains("deprecated"), "{}", text(&out));
    let _ = std::fs::remove_dir_all(&slots); // best-effort: scratch
}

/// `--now` runs at once beside a holder, unqueued, and journals `now`.
#[test]
fn now_skips_the_queue_and_is_journalled() {
    let slots = slot_dir("now");
    let mut holder = spawn(&slots, &["slot", "run", "--", "sleep", "4"]);
    until_held(&slots);
    let marker = slots.join("ran");
    let started = Instant::now();
    let out = amont(
        &slots,
        &[
            "slot",
            "run",
            "--now",
            "--",
            "touch",
            marker.to_str().unwrap(),
        ],
    )
    .output()
    .expect("now");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(marker.exists());
    assert!(started.elapsed() < Duration::from_secs(3), "--now waited");
    let waits = std::fs::read_to_string(slots.join("waits.jsonl")).unwrap_or_default();
    assert!(waits.contains("\"outcome\":\"now\""), "{waits}");
    let _ = holder.wait(); // best-effort: reap the holder
    let _ = std::fs::remove_dir_all(&slots); // best-effort: scratch
}

/// A wrapper under a held slot does not queue for the slot its parent holds,
/// and the wrapped command's own flags are its own.
#[test]
fn a_nested_wrapper_runs_at_once_and_flags_after_dashes_pass_through() {
    let slots = slot_dir("nested");
    // The holder's command records the marker it was given: the child below
    // carries exactly that, as a real child inherits it.
    let given = slots.join("marker");
    let record = format!(
        "printf %s \"$GREENHARNESS_HOST_SLOT\" > '{}'; sleep 3",
        given.display()
    );
    let mut holder = spawn(&slots, &["slot", "run", "--", "sh", "-c", &record]);
    until_held(&slots);
    let deadline = Instant::now() + Duration::from_secs(30);
    let marker = loop {
        let marker = std::fs::read_to_string(&given).unwrap_or_default();
        if !marker.is_empty() {
            break marker;
        }
        assert!(Instant::now() < deadline, "the holder exported no marker");
        std::thread::sleep(Duration::from_millis(20));
    };
    // The holder exports its own lease (t15 phase 2), the one its record names.
    let lease = marker.strip_prefix("held:").expect("a held:<lease> marker");
    let records: String = std::fs::read_dir(slots.join("holders"))
        .expect("holders")
        .filter_map(|e| std::fs::read_to_string(e.ok()?.path()).ok())
        .collect();
    assert!(records.contains(lease), "{marker} not in {records}");
    let started = Instant::now();
    let out = amont(
        &slots,
        &[
            "slot",
            "run",
            "--",
            "sh",
            "-c",
            "echo got:\"$1\" m:\"$GREENHARNESS_HOST_SLOT\"",
            "x",
            "--help",
        ],
    )
    .env("GREENHARNESS_HOST_SLOT", &marker)
    .output()
    .expect("nested");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(text(&out).contains("got:--help"), "{}", text(&out));
    // A nested wrapper passes the inherited marker through untouched.
    assert!(
        text(&out).contains(&format!("m:{marker}")),
        "{}",
        text(&out)
    );
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "a nested wrapper queued"
    );
    let _ = holder.wait(); // best-effort: reap the holder
    let _ = std::fs::remove_dir_all(&slots); // best-effort: scratch
}

/// An accepted bare `held` (an older holder) runs at once, says nothing,
/// and leaves one `marker_bare` row with no `waited_ms` and no `reason`, so
/// the phase-3 gate of t15 can count the writers still bare.
#[test]
fn a_bare_marker_runs_at_once_and_is_counted_once() {
    let slots = slot_dir("bare");
    let out = amont(&slots, &["slot", "run", "--", "true"])
        .env("GREENHARNESS_HOST_SLOT", "held")
        .output()
        .expect("bare");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(
        !text(&out).contains("GREENHARNESS_HOST_SLOT"),
        "{}",
        text(&out)
    );
    let waits = std::fs::read_to_string(slots.join("waits.jsonl")).unwrap_or_default();
    let rows: Vec<&str> = waits.lines().collect();
    assert_eq!(rows.len(), 1, "{waits}");
    assert!(rows[0].ends_with("\"outcome\":\"marker_bare\"}"), "{waits}");
    assert!(!rows[0].contains("waited_ms"), "{waits}");
    assert!(!rows[0].contains("\"reason\""), "{waits}");
    let _ = std::fs::remove_dir_all(&slots); // best-effort: scratch
}

/// A marker that names no live lease (`=1`, set by hand) queues like none:
/// one refusal line, one `marker_refused` row, then a grant, and the exit
/// status stays the command's. With slots off, nothing is said or written.
#[test]
fn a_refused_marker_queues_and_says_so_once() {
    let slots = slot_dir("refused");
    let out = amont(&slots, &["slot", "run", "--", "sh", "-c", "exit 7"])
        .env("GREENHARNESS_HOST_SLOT", "1")
        .output()
        .expect("refused");
    assert_eq!(out.status.code(), Some(7), "{}", text(&out));
    let said = text(&out);
    assert_eq!(
        said.matches("GREENHARNESS_HOST_SLOT=1 names no live lease (malformed); queueing")
            .count(),
        1,
        "{said}"
    );
    assert!(
        said.contains("unset it; only a slot holder sets it"),
        "{said}"
    );
    let waits = std::fs::read_to_string(slots.join("waits.jsonl")).unwrap_or_default();
    assert_eq!(
        waits.matches("\"outcome\":\"marker_refused\"").count(),
        1,
        "{waits}"
    );
    assert!(waits.contains("\"reason\":\"malformed\""), "{waits}");
    assert_eq!(
        waits.matches("\"outcome\":\"admitted\"").count(),
        1,
        "{waits}"
    );

    let off = slot_dir("refused-off");
    let out = amont(&off, &["slot", "run", "--", "true"])
        .env("GREENHARNESS_HOST_SLOT", "1")
        .env("GREENGATE_HOST_SLOTS", "0")
        .output()
        .expect("slots off");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(
        !text(&out).contains("names no live lease"),
        "{}",
        text(&out)
    );
    assert!(!off.join("waits.jsonl").exists());
    let _ = std::fs::remove_dir_all(&slots); // best-effort: scratch
    let _ = std::fs::remove_dir_all(&off); // best-effort: scratch
}

/// The wrapper alone holds the slot: a daemon the command leaves behind
/// (`setsid`, detached) does not keep it once the command exits.
#[test]
fn a_detached_daemon_does_not_keep_the_slot() {
    let slots = slot_dir("daemon");
    let out = amont(
        &slots,
        &[
            "slot",
            "run",
            "--",
            "sh",
            "-c",
            "(sleep 20 >/dev/null 2>&1 &); exit 0",
        ],
    )
    .output()
    .expect("daemon");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let started = Instant::now();
    let next = amont(&slots, &["slot", "run", "--max-wait", "5", "--", "true"])
        .output()
        .expect("next");
    assert_eq!(next.status.code(), Some(0), "{}", text(&next));
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "the daemon held the slot"
    );
    let _ = std::fs::remove_dir_all(&slots); // best-effort: scratch
}

const WIDTH_KEYS: [&str; 6] = [
    "MAKEFLAGS",
    "CARGO_BUILD_JOBS",
    "RUST_TEST_THREADS",
    "NEXTEST_TEST_THREADS",
    "VITEST_MAX_WORKERS",
    "GOMAXPROCS",
];

/// `amont slot run -- env` with `cap` slots, the width keys cleared first
/// (a `make check` parent exports its own MAKEFLAGS), then `preset` set.
fn env_under(slots: &Path, cap: &str, flags: &[&str], preset: &[(&str, &str)]) -> String {
    let mut args = vec!["slot", "run"];
    args.extend_from_slice(flags);
    args.extend_from_slice(&["--", "env"]);
    let mut cmd = amont(slots, &args);
    cmd.env("GREENGATE_HOST_SLOTS", cap);
    for key in WIDTH_KEYS {
        cmd.env_remove(key);
    }
    for (k, v) in preset {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("slot run env");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn var<'a>(env: &'a str, key: &str) -> Option<&'a str> {
    env.lines()
        .find_map(|l| l.strip_prefix(key).and_then(|r| r.strip_prefix('=')))
}

/// The core count the width tests pin through `GREENGATE_HOST_CORES`.
fn cores() -> u32 {
    8
}

/// A held command runs `weight × cores / cap` wide, through every key,
/// and `amont slots` shows the width beside the holder.
#[test]
fn a_held_command_is_bounded_to_its_share_of_the_cores() {
    let slots = slot_dir("width");
    // 8 cores pinned, 4 slots: weight 1 runs 2 wide, weight 3 runs 6 wide.
    for (weight, jobs) in [(1u32, 2u32), (3, 6)] {
        let env = env_under(&slots, "4", &["--weight", &weight.to_string()], &[]);
        assert_eq!(
            var(&env, "CARGO_BUILD_JOBS"),
            Some(jobs.to_string().as_str()),
            "{env}"
        );
        assert_eq!(
            var(&env, "RUST_TEST_THREADS"),
            Some(jobs.to_string().as_str()),
            "{env}"
        );
        assert_eq!(
            var(&env, "NEXTEST_TEST_THREADS"),
            Some(jobs.to_string().as_str()),
            "{env}"
        );
        assert_eq!(
            var(&env, "VITEST_MAX_WORKERS"),
            Some(jobs.to_string().as_str()),
            "{env}"
        );
        assert_eq!(
            var(&env, "GOMAXPROCS"),
            Some(jobs.to_string().as_str()),
            "{env}"
        );
        assert_eq!(
            var(&env, "MAKEFLAGS"),
            Some(format!("-j{jobs}").as_str()),
            "{env}"
        );
    }
    let holder = spawn(&slots, &["slot", "run", "--", "sleep", "3"]);
    let json = until_held(&slots);
    assert!(
        json.contains(&format!("\"jobs\":{}", cores().max(1))),
        "{json}"
    );
    let out = amont(&slots, &["slots"]).output().expect("amont slots");
    assert!(
        text(&out).contains(&format!("jobs {}", cores().max(1))),
        "{}",
        text(&out)
    );
    let _ = holder.wait_with_output(); // best-effort: reap the holder
    let _ = std::fs::remove_dir_all(&slots); // best-effort: scratch
}

/// The command's own setting wins, an empty one is replaced, and a
/// MAKEFLAGS with flags but no job bound gains one.
#[test]
fn a_preset_width_wins_and_an_empty_one_does_not() {
    let slots = slot_dir("preset");
    let env = env_under(
        &slots,
        "4",
        &[],
        &[
            ("CARGO_BUILD_JOBS", "9"),
            ("RUST_TEST_THREADS", ""),
            ("MAKEFLAGS", "--no-print-directory"),
        ],
    );
    let jobs = 2; // 8 cores pinned, 4 slots, weight 1
    assert_eq!(var(&env, "CARGO_BUILD_JOBS"), Some("9"), "{env}");
    assert_eq!(
        var(&env, "RUST_TEST_THREADS"),
        Some(jobs.to_string().as_str()),
        "{env}"
    );
    assert_eq!(
        var(&env, "MAKEFLAGS"),
        Some(format!("--no-print-directory -j{jobs}").as_str()),
        "{env}"
    );
    let _ = std::fs::remove_dir_all(&slots); // best-effort: scratch
}

/// A `--now` command runs unqueued, and is bounded too: that happens when the
/// host is busy.
#[test]
fn a_now_command_is_bounded_too() {
    let slots = slot_dir("unqueued");
    let first = spawn(&slots, &["slot", "run", "--", "sleep", "5"]);
    until_held(&slots);
    let env = env_under(&slots, "1", &["--now"], &[]);
    assert_eq!(
        var(&env, "CARGO_BUILD_JOBS"),
        Some(cores().max(1).to_string().as_str()),
        "{env}"
    );
    let _ = first.wait_with_output(); // best-effort: reap the holder
    let _ = std::fs::remove_dir_all(&slots); // best-effort: scratch
}

/// `amont` run as one agent session: the runner's own session variables
/// removed, then one set, so the test's session never leaks in.
fn as_session(slots: &Path, session: &str, args: &[&str]) -> Command {
    let mut cmd = amont(slots, args);
    cmd.env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("RELAIS_SESSION_ID")
        .env_remove("CLAUDE_SESSION_ID")
        .env("CLAUDE_CODE_SESSION_ID", session);
    cmd
}

/// A child's stderr, read on a thread as it is written, so a test can wait
/// for a line before it ends what the child waits on.
struct Said {
    bytes: Arc<Mutex<Vec<u8>>>,
    reader: std::thread::JoinHandle<()>,
}

fn said_by(child: &mut Child) -> Said {
    use std::io::{ErrorKind, Read as _};
    let mut err = child.stderr.take().expect("stderr is piped");
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&bytes);
    let reader = std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match err.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => sink.lock().unwrap().extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
    });
    Said { bytes, reader }
}

impl Said {
    /// Wait until the child has said `want`, at most 30 s.
    fn until(&self, want: &str) {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let said = String::from_utf8_lossy(&self.bytes.lock().unwrap()).into_owned();
            if said.contains(want) {
                return;
            }
            assert!(Instant::now() < deadline, "never said {want:?}: {said}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Everything the child said, once its stderr has closed.
    fn all(self) -> String {
        self.reader.join().expect("stderr reader");
        let bytes = std::mem::take(&mut *self.bytes.lock().unwrap());
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

/// The `slots --json` queue, parsed.
fn queue_of(slots: &Path) -> Vec<serde_json::Value> {
    let out = amont(slots, &["slots", "--json"]).output().expect("slots");
    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("slots --json is JSON");
    json["queue"].as_array().cloned().unwrap_or_default()
}

/// t13 on the built binary: two jobs wait behind a held slot, the queue
/// names their sessions, another session's move is refused by name, and the
/// job's own session moves it to the head with a journalled row.
#[test]
fn a_session_moves_its_own_waiting_job_to_the_head() {
    let slots = slot_dir("promote");
    // The holder holds until the test says so, never on a timer: on a busy
    // host the steps below can outlast any sleep.
    let release = slots.join("release-holder");
    let release_arg = release.to_string_lossy().into_owned();
    // Releases the holder on every exit, a failed assertion included, so a
    // red run does not leave its holder looping on a scratch file.
    struct Release(PathBuf);
    impl Drop for Release {
        fn drop(&mut self) {
            let _ = std::fs::write(&self.0, ""); // best-effort: the test is over
        }
    }
    let _release_on_exit = Release(release.clone());
    let first = spawn(
        &slots,
        &[
            "slot",
            "run",
            "--",
            "sh",
            "-c",
            "while [ ! -e \"$0\" ]; do sleep 0.1; done",
            &release_arg,
        ],
    );
    until_held(&slots);
    let theirs = as_session(&slots, "session-theirs", &["slot", "run", "--", "true"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("theirs");
    until_waiting(&slots, 1);
    let mut mine = as_session(&slots, "session-mine", &["slot", "run", "--", "true"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("mine");
    let mine_said = said_by(&mut mine);
    until_waiting(&slots, 2);
    let queue = queue_of(&slots);
    assert_eq!(queue.len(), 2, "{queue:?}");
    assert_eq!(queue[0]["session"], "session-theirs", "{queue:?}");
    assert_eq!(queue[1]["session"], "session-mine", "{queue:?}");
    assert_eq!(queue[1]["position"], 1, "{queue:?}");
    let id = queue[1]["id"].as_str().expect("an id").to_string();

    let refused = as_session(
        &slots,
        "session-theirs",
        &["slot", "promote", &id, "--reason", "mine first"],
    )
    .output()
    .expect("promote");
    assert_eq!(refused.status.code(), Some(1), "{}", text(&refused));
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("refused (own-session)"),
        "{}",
        text(&refused)
    );

    let moved = as_session(
        &slots,
        "session-mine",
        &["slot", "promote", &id, "--reason", "the person waits on it"],
    )
    .output()
    .expect("promote");
    assert_eq!(moved.status.code(), Some(0), "{}", text(&moved));
    // Every message goes to stderr: the exit status is the result.
    assert!(
        String::from_utf8_lossy(&moved.stderr).starts_with(&format!(
            "moved {id} from 1 to 0, past 1 job; 2 of 3 moves left"
        )),
        "{}",
        text(&moved)
    );
    assert!(moved.stdout.is_empty(), "{}", text(&moved));
    let queue = queue_of(&slots);
    assert_eq!(queue[0]["id"], id.as_str(), "{queue:?}");
    let waits = std::fs::read_to_string(slots.join("waits.jsonl")).unwrap_or_default();
    assert_eq!(
        waits.matches("\"outcome\":\"promoted\"").count(),
        1,
        "{waits}"
    );
    assert!(
        waits.contains("\"reason\":\"the person waits on it\""),
        "{waits}"
    );
    assert!(waits.contains("\"limit\":\"own-session\""), "{waits}");

    // The waiter says its id on a poll that finds the holder, and polls
    // every 200 ms. Its first poll can answer an empty queue (it missed the
    // admission lock that this test's `slots --json` calls hold), and the
    // steps above can end before the second. A holder released on sight then
    // grants the waiter silently, so release it only once the line is said.
    mine_said.until(&format!("id {id}; move ahead: greengate slot promote {id}"));
    std::fs::write(&release, "").expect("release the holder");
    let mine = mine.wait().expect("mine");
    let said = mine_said.all();
    let theirs = theirs.wait_with_output().expect("theirs");
    assert_eq!(
        (mine.code(), theirs.status.code()),
        (Some(0), Some(0)),
        "{said}"
    );
    let _ = first.wait_with_output(); // best-effort: reap the holder
    let _ = std::fs::remove_dir_all(&slots); // best-effort: scratch
}

/// The off switch on the built binary: the environment, then the host git
/// key, each refuses by name with its source and journals one row; an
/// environment `1` never undoes a host `0`.
#[test]
fn promote_off_refuses_by_name_from_each_source() {
    let slots = slot_dir("promote-off");
    let run = |env: &[(&str, &str)], gitconfig: Option<&str>| {
        let mut cmd = as_session(
            &slots,
            "session-mine",
            &["slot", "promote", "1-0", "--reason", "mine first"],
        );
        cmd.env_remove("GREENGATE_HOST_PROMOTE");
        if let Some(body) = gitconfig {
            let file = slots.join("gitconfig");
            std::fs::write(&file, body).unwrap();
            cmd.env("GIT_CONFIG_GLOBAL", &file);
        }
        for (k, v) in env {
            cmd.env(k, v);
        }
        cmd.output().expect("promote")
    };

    let by_env = run(&[("GREENGATE_HOST_PROMOTE", "0")], None);
    let said = String::from_utf8_lossy(&by_env.stderr).into_owned();
    assert_eq!(by_env.status.code(), Some(1), "{said}");
    assert!(said.contains("refused (off): promotion is off"), "{said}");
    assert!(said.contains("set by: GREENGATE_HOST_PROMOTE=0"), "{said}");
    assert!(by_env.stdout.is_empty(), "{said}");

    let by_git = run(&[], Some("[greengate]\n\thostPromote = 0\n"));
    let said = String::from_utf8_lossy(&by_git.stderr).into_owned();
    assert_eq!(by_git.status.code(), Some(1), "{said}");
    assert!(
        said.contains("set by: git config --global greengate.hostPromote 0"),
        "{said}"
    );

    // An environment 1 does not undo it, and says so once.
    let forced = run(
        &[("GREENGATE_HOST_PROMOTE", "1")],
        Some("[greengate]\n\thostPromote = 0\n"),
    );
    let said = String::from_utf8_lossy(&forced.stderr).into_owned();
    assert_eq!(forced.status.code(), Some(1), "{said}");
    assert!(said.contains("does not turn promotion back on"), "{said}");

    // Unset, promotion follows the crate's rules (no such job here).
    let on = run(&[], Some("[greengate]\n\thostPromote = 1\n"));
    let said = String::from_utf8_lossy(&on.stderr).into_owned();
    assert!(said.contains("refused (not-queued)"), "{said}");

    let waits = std::fs::read_to_string(slots.join("waits.jsonl")).unwrap_or_default();
    assert_eq!(waits.matches("\"limit\":\"off\"").count(), 3, "{waits}");
    assert!(
        waits.contains("\"limit\":\"off\",\"source\":\"env\""),
        "{waits}"
    );
    assert!(
        waits.contains("\"limit\":\"off\",\"source\":\"global\""),
        "{waits}"
    );
    let _ = std::fs::remove_dir_all(&slots); // best-effort: scratch
}

/// A promote refused before the queue is touched creates no slot directory,
/// and a word after `--` is the job.
#[test]
fn a_bad_promote_creates_no_slot_directory() {
    let slots = slot_dir("promote-bad").join("never-made");
    let bad = as_session(
        &slots,
        "session-mine",
        &["slot", "promote", "1-0", "--reason", "  "],
    )
    .output()
    .expect("promote");
    assert_eq!(bad.status.code(), Some(2), "{}", text(&bad));
    let off = as_session(&slots, "", &["slot", "promote", "1-0", "--reason", "r"])
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .output()
        .expect("promote");
    assert_eq!(off.status.code(), Some(1), "{}", text(&off));
    assert!(
        String::from_utf8_lossy(&off.stderr).contains("refused (no-session)"),
        "{}",
        text(&off)
    );
    assert!(!slots.exists(), "no slot directory may appear");

    // After `--` the operand is the job, even when it looks like a flag.
    let made = slot_dir("promote-dashes");
    let after = as_session(
        &made,
        "session-mine",
        &["slot", "promote", "--reason", "r", "--", "7-7"],
    )
    .output()
    .expect("promote");
    assert_eq!(after.status.code(), Some(1), "{}", text(&after));
    assert!(
        String::from_utf8_lossy(&after.stderr).contains("no waiting job 7-7"),
        "{}",
        text(&after)
    );
    let _ = std::fs::remove_dir_all(&made); // best-effort: scratch
}
