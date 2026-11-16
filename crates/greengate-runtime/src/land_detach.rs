//! `land --detach`, its hidden worker, and `land --wait`.
//!
//! The launcher judges `HEAD`, writes the receipt once as `running`, starts
//! a worker that holds the receipt's `flock`, and returns. The worker pushes
//! only the judged sha, never retries, and always finalizes the receipt.
//! `--wait` reads the receipt: a `running` receipt whose lock is free belongs
//! to a dead worker (a pid is reused; a lock is not).

use crate::json_read::{self, Value};
use crate::land::{
    finish, lock_held, now_nanos, receipt_body, receipt_dir, short, shown_ref, verdict_word,
    write_atomic, Meta, Options, Report, Status,
};
use crate::names::BIN;
use crate::posix_signal::{libc_pipe, libc_read, libc_signal, libc_write, SIGHUP, SIGINT, SIGTERM};
use std::fs::File;
use std::os::unix::io::FromRawFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// How often `--wait` reads the receipt.
const WAIT_POLL: Duration = Duration::from_secs(2);

/// How long a launch waits for another launch's check to end.
const LAUNCH_LOCK_BUDGET: Duration = Duration::from_secs(10);

/// Exit code of `--wait` when the worker died without finalizing.
const WORKER_DIED: i32 = 3;

/// `word` single-quoted, so it pastes into any shell.
fn quoted(word: &str) -> String {
    format!("'{}'", word.replace('\'', "'\\''"))
}

/// `word` as one shell word, single-quoted when it needs it.
fn shell_word(word: &str) -> String {
    let plain = !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./=:".contains(c));
    if plain {
        word.to_string()
    } else {
        format!("'{}'", word.replace('\'', "'\\''"))
    }
}

fn wait_command(receipt: &Path) -> String {
    format!("{BIN} land --wait {}", quoted(&receipt.to_string_lossy()))
}

fn lock_path(receipt: &Path) -> PathBuf {
    let mut lock = receipt.as_os_str().to_owned();
    lock.push(".lock");
    PathBuf::from(lock)
}

/// What `--wait` and the worker read back from a receipt.
struct View {
    status: String,
    exit: Option<i32>,
    branch: String,
    dst: String,
    remote_before: String,
    remote_after: String,
    started: i64,
    judged: String,
    log: String,
    worker_pid: Option<u32>,
    flags: Vec<String>,
}

fn text(doc: &Value, key: &str) -> String {
    doc.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn number(doc: &Value, key: &str) -> Option<i64> {
    match doc.get(key) {
        Some(Value::Num(n)) => n.parse().ok(),
        _ => None,
    }
}

fn read_view(path: &Path) -> Option<View> {
    let doc = json_read::parse(std::fs::read_to_string(path).ok()?.trim())?;
    Some(View {
        status: doc.get("status")?.as_str()?.to_string(),
        exit: number(&doc, "exit").and_then(|n| i32::try_from(n).ok()),
        branch: text(&doc, "branch"),
        dst: text(&doc, "dst"),
        remote_before: text(&doc, "remote_before"),
        remote_after: text(&doc, "remote_after"),
        started: number(&doc, "started").unwrap_or(0),
        judged: text(&doc, "judged"),
        log: text(&doc, "log"),
        worker_pid: number(&doc, "worker_pid")
            .and_then(|n| u32::try_from(n).ok())
            .filter(|pid| *pid != 0),
        flags: doc
            .get("flags")
            .map(|f| {
                f.items()
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
    })
}

/// The status word a final receipt carries, as the exit code it stands for.
fn code_of_word(word: &str) -> i32 {
    match word {
        "landed" => 0,
        "usage" => 2,
        "unreachable" | "unverified" => 3,
        "not-observed" => 4,
        "interrupted" => 130,
        _ => 1,
    }
}

fn judged_head(tag: Option<&str>) -> String {
    let args = match tag {
        Some(name) => vec![
            "rev-parse".to_string(),
            "--verify".to_string(),
            "-q".to_string(),
            format!("refs/tags/{name}^{{commit}}"),
        ],
        None => vec!["rev-parse".to_string(), "HEAD".to_string()],
    };
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    crate::git::stdout(&args).unwrap_or_default()
}

/// The receipt a land has until its worker takes it over: what the launcher
/// knows without resolving anything.
fn skeleton(options: &Options) -> Report {
    let mut report = Report::new(Status::Refused);
    match &options.tag {
        Some(name) => {
            report.dst = format!("refs/tags/{name}");
            report.branch = report.dst.clone();
        }
        None => {
            if let Some(branch) =
                crate::git::stdout(&["symbolic-ref", "--quiet", "--short", "HEAD"])
            {
                report.dst = format!("refs/heads/{branch}");
                report.branch = branch;
            }
        }
    }
    report
}

fn refused_launch(why: &str) -> i32 {
    eprintln!("{BIN}: land: {why}");
    1
}

/// `land --detach`: judge, record, start the worker, print where to wait.
pub(crate) fn launch(options: &Options) -> i32 {
    let Some(dir) = receipt_dir() else {
        return refused_launch("could not find the git directory");
    };
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return refused_launch(&format!("could not create {}: {e}", dir.display()));
    }
    // The check for a live worker, the receipt and the spawn are one step:
    // two launches that pass the check together would both start.
    let launch_lock = match std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join("land.lock"))
    {
        Ok(file) => file,
        Err(e) => return refused_launch(&format!("could not open the launch lock: {e}")),
    };
    if !crate::fdlock::exclusive_within(&launch_lock, LAUNCH_LOCK_BUDGET) {
        return refused_launch("another launch is in progress; try again");
    }
    if let Some(live) = live_receipt(&dir) {
        eprintln!(
            "{BIN}: land: a detached land is already running: {}\n    {}",
            live.display(),
            wait_command(&live)
        );
        return 1;
    }
    crate::land::prune_receipts(&dir);

    let judged = judged_head(options.tag.as_deref());
    let started = now_nanos();
    let receipt = dir.join(format!("{started}-{}.json", std::process::id()));
    let log = receipt.with_extension("log");
    let lock = match std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(lock_path(&receipt))
    {
        Ok(file) => file,
        Err(e) => return refused_launch(&format!("could not open the receipt lock: {e}")),
    };
    if !crate::fdlock::try_exclusive(&lock) {
        return refused_launch("the receipt lock is already held");
    }
    let report = skeleton(options);
    let meta = Meta {
        judged: judged.clone(),
        log: log.to_string_lossy().into_owned(),
        worker_pid: None,
        flags: options.rerun_flags(),
    };
    let body = receipt_body(&report, "running", None, (started, None), Some(&meta));
    if let Err(e) = write_atomic(&receipt, &body) {
        return refused_launch(&format!(
            "could not write the receipt {}: {e}",
            receipt.display()
        ));
    }

    let fd = std::os::unix::io::AsRawFd::as_raw_fd(&lock);
    let mut worker_args: Vec<String> = vec![
        "land".into(),
        "--worker".into(),
        "--receipt".into(),
        receipt.to_string_lossy().into_owned(),
        "--lock-fd".into(),
        fd.to_string(),
    ];
    worker_args.extend(options.rerun_flags());
    let worker_args: Vec<&str> = worker_args.iter().map(String::as_str).collect();
    if let Err(e) = crate::rehearsal::spawn_amont(&worker_args, &log, Some(fd)) {
        let mut failed = report;
        failed.status = Status::Refused;
        let body = receipt_body(
            &failed,
            failed.status.as_str(),
            Some(failed.status.code()),
            (started, Some(now_nanos())),
            Some(&meta),
        );
        if let Err(write_error) = write_atomic(&receipt, &body) {
            eprintln!("{BIN}: land: could not finalize the receipt: {write_error}");
        }
        return refused_launch(&format!("could not start the worker: {e}"));
    }
    drop(launch_lock);

    let wait = wait_command(&receipt);
    if options.json {
        println!(
            "{}",
            crate::json::object(&[
                crate::json::string_field("status", "detached"),
                crate::json::string_field("receipt", &receipt.to_string_lossy()),
                crate::json::string_field("log", &meta.log),
                crate::json::string_field("wait", &wait),
            ])
        );
    } else {
        println!("{wait}");
    }
    eprintln!("detached {} {}", short(&judged), shown_ref(&report));
    eprintln!("not landed yet: wait on the receipt");
    0
}

/// A `running` receipt of this worktree whose worker is alive.
fn live_receipt(dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    entries.flatten().map(|e| e.path()).find(|path| {
        path.extension().is_some_and(|e| e == "json")
            && read_view(path).is_some_and(|view| view.status == "running")
            && lock_held(path)
    })
}

/// The receipt's last writer: the worker, once, or the signal watcher.
struct Finalizer {
    path: PathBuf,
    started: i64,
    meta: Meta,
    skeleton: Report,
    done: AtomicBool,
}

impl Finalizer {
    fn write(&self, report: &Report) {
        if self.done.swap(true, Ordering::SeqCst) {
            return;
        }
        let mut report = report.clone();
        if report.branch.is_empty() && report.dst.is_empty() {
            report.branch = self.skeleton.branch.clone();
            report.dst = self.skeleton.dst.clone();
        }
        let body = receipt_body(
            &report,
            report.status.as_str(),
            Some(report.status.code()),
            (self.started, Some(now_nanos())),
            Some(&self.meta),
        );
        if let Err(e) = write_atomic(&self.path, &body) {
            eprintln!(
                "{BIN}: land: could not write the receipt {}: {e}",
                self.path.display()
            );
        }
    }
}

static SIGNAL_PIPE_WRITE: AtomicI32 = AtomicI32::new(-1);
static PENDING_SIGNAL: AtomicI32 = AtomicI32::new(0);

extern "C" fn on_signal(sig: i32) {
    // Stores and one write: the watcher thread does the rest.
    PENDING_SIGNAL.store(sig, Ordering::SeqCst);
    let fd = SIGNAL_PIPE_WRITE.load(Ordering::SeqCst);
    if fd >= 0 {
        let byte = 1u8;
        // SAFETY: write(2) of one byte is async-signal-safe.
        unsafe {
            libc_write(fd, &byte as *const u8, 1);
        }
    }
}

/// On SIGTERM, SIGINT or SIGHUP: finalize `interrupted`, stop the worker's
/// children, exit 128+n.
fn finalize_on_signals(fin: Arc<Finalizer>) {
    let mut fds = [0i32; 2];
    // SAFETY: pipe(2) fills two descriptors.
    if unsafe { libc_pipe(fds.as_mut_ptr()) } != 0 {
        eprintln!("{BIN}: land: no signal pipe; a signal will not finalize the receipt");
        return;
    }
    SIGNAL_PIPE_WRITE.store(fds[1], Ordering::SeqCst);
    let read_fd = fds[0];
    std::thread::spawn(move || {
        let mut byte = 0u8;
        loop {
            // SAFETY: read(2) into one owned byte.
            let n = unsafe { libc_read(read_fd, &mut byte as *mut u8, 1) };
            if n > 0 {
                break;
            }
            if n == 0 {
                return;
            }
            // -1: interrupted by a signal; read again.
        }
        let sig = PENDING_SIGNAL.load(Ordering::SeqCst);
        let mut report = fin.skeleton.clone();
        report.status = Status::Interrupted(sig);
        fin.write(&report);
        crate::fdlock::kill_group(std::process::id() as i32, SIGTERM);
        std::process::exit(128 + sig);
    });
    // SAFETY: `on_signal` only stores and writes.
    unsafe {
        for sig in [SIGHUP, SIGINT, SIGTERM] {
            libc_signal(sig, on_signal as *const () as usize);
        }
    }
}

/// The hidden `land --worker`: take the receipt, land the judged sha, finalize.
pub(crate) fn worker(options: &Options) -> i32 {
    let (Some(receipt), Some(lock_fd)) = (&options.receipt, options.lock_fd) else {
        return 2;
    };
    // First: nothing this worker starts may hold the lock after it dies.
    crate::fdlock::set_cloexec(lock_fd, true);
    // SAFETY: the launcher passed this descriptor, an open lock file, and
    // nothing else in this process owns it.
    let _lock = unsafe { File::from_raw_fd(lock_fd) };
    let path = PathBuf::from(receipt);
    let Some(view) = read_view(&path) else {
        eprintln!("{BIN}: land: cannot read the receipt {receipt}");
        return 1;
    };
    let mut skeleton = Report::new(Status::Refused);
    skeleton.branch = view.branch.clone();
    skeleton.dst = view.dst.clone();
    let meta = Meta {
        judged: view.judged.clone(),
        log: view.log.clone(),
        worker_pid: Some(std::process::id()),
        flags: view.flags.clone(),
    };
    let body = receipt_body(
        &skeleton,
        "running",
        None,
        (view.started, None),
        Some(&meta),
    );
    if let Err(e) = write_atomic(&path, &body) {
        eprintln!("{BIN}: land: could not take over the receipt {receipt}: {e}");
    }
    let fin = Arc::new(Finalizer {
        path,
        started: view.started,
        meta,
        skeleton,
        done: AtomicBool::new(false),
    });
    finalize_on_signals(Arc::clone(&fin));

    let report = match &options.tag {
        Some(name) => crate::land::land_tag(name, Some(&view.judged)),
        None => crate::land::land(options.mode(), Some(&view.judged)),
    };
    fin.write(&report);
    finish(&report, options.json)
}

/// `land --wait <receipt>`.
pub(crate) fn wait(receipt: &str) -> i32 {
    let path = Path::new(receipt);
    let Some(first) = read_view(path) else {
        eprintln!("{BIN}: land: no readable receipt at {receipt}");
        return 2;
    };
    eprintln!("waiting on {receipt} (log: {})", first.log);
    loop {
        let Some(view) = read_view(path) else {
            eprintln!("{BIN}: land: the receipt {receipt} is gone or unreadable");
            return 2;
        };
        if view.status != "running" {
            let shown = if view.branch.is_empty() {
                view.dst.strip_prefix("refs/tags/").unwrap_or(&view.dst)
            } else {
                &view.branch
            };
            println!(
                "{}",
                verdict_word(&view.status, &view.remote_before, &view.remote_after, shown)
            );
            return view.exit.unwrap_or_else(|| code_of_word(&view.status));
        }
        if !lock_held(path) {
            // The worker may have finalized between the read and the lock.
            match read_view(path) {
                Some(again) if again.status == "running" => return died(&again),
                _ => continue,
            }
        }
        std::thread::sleep(crate::clock::scaled(WAIT_POLL));
    }
}

fn died(view: &View) -> i32 {
    let pid = view
        .worker_pid
        .map_or_else(|| "unknown".to_string(), |p| p.to_string());
    println!("worker {pid} died");
    println!("  log: {}", quoted(&view.log));
    let flags: Vec<String> = view.flags.iter().map(|f| shell_word(f)).collect();
    println!("  rerun: {BIN} land --detach {}", flags.join(" "));
    WORKER_DIED
}
