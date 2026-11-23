//! Host slots: one machine-wide queue for every heavy job on the host
//! (amont ADR-0014, decisions ADR-0049). The contract is `docs/host-slots.md`.
//!
//! A heavy job — a test suite, a build, a type checker — already uses every
//! core. Several of them at once, from several worktrees, sessions and
//! agents, thrash the same cores until their own timeouts fire. So each takes
//! a slot first: a `flock` on `slot-N` under one fixed per-user directory.
//!
//! What lives in the directory:
//!
//! ```text
//! slot-0 … slot-{cap-1}   the slots; held = flocked
//! admission               the one admission lock (taken without blocking)
//! admitted-at             when the last holder was admitted, for the ramp
//! queue/<nanos>-<pid>     a waiter's ticket, flocked for as long as it waits
//! holders/slot-N.who      who holds slot N (JSON), while it does
//! waits.jsonl             one line per acquisition, and one per released
//!                         lease, for measurement
//! ```
//!
//! Only the process that took a slot holds it: every descriptor here is
//! opened close-on-exec (Rust's default), so a daemon a build leaves behind
//! never keeps a slot.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub mod host;

/// Exported to the command a slot holder runs; an amont (or a wrapper)
/// started under it does not queue again for the slot its parent holds.
pub const HELD_ENV: &str = "GREENHARNESS_HOST_SLOT";

/// A diagnostic, and the seam tests use: the slot directory.
pub const DIR_ENV: &str = "GREENHARNESS_SLOT_DIR";

/// At most one new holder per this long under automatic sizing: the
/// one-minute load average lags, and after 10 s shows only ~15% of a new
/// holder's load, so a burst of waiters on an idle host would all pile in.
pub const RAMP: Duration = Duration::from_secs(60);

/// How much one-minute load each running job of a holder brings, for the
/// load test (ADR-0022): a further slot is taken while `load1 < cores +
/// LOAD_PER_JOB × Σjobs(holders)`. One holder at `jobs 2` measured 12.3
/// against an idle baseline of about 4 (t01), so about 4 per job; counting
/// only the cores, the holders' own load kept every other slot shut.
pub const LOAD_PER_JOB: u32 = 4;

/// A wait whose line made no progress for this long has stalled (ADR-0022):
/// no admission anywhere and no change of this waiter's position.
pub const STALL: Duration = Duration::from_secs(300);

/// A test seam beside [`DIR_ENV`]: the stall window in whole seconds.
pub const STALL_ENV: &str = "GREENHARNESS_SLOT_STALL_S";

/// How many slots THIS process holds, across its threads.
static HELD: AtomicUsize = AtomicUsize::new(0);

/// Slots this process holds right now.
pub fn held_in_process() -> usize {
    HELD.load(Ordering::SeqCst)
}

/// How many slots there are, and whether the load decides admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sizing {
    /// `greengate.hostSlots` unset: at most `max(1, cores/2)`; a free slot is
    /// taken only while the one-minute load is below the core count, at most
    /// one new holder per [`RAMP`].
    Auto,
    /// `greengate.hostSlots N`: exactly N, no load test, no ramp.
    Fixed(u32),
}

impl Sizing {
    /// How many slot files count on this machine.
    pub fn cap(self) -> u32 {
        self.cap_for(cores())
    }

    /// How many slot files count on a machine with `cores` cores: the one
    /// admission reads, so the cap and the load test see the same machine.
    pub fn cap_for(self, cores: u32) -> u32 {
        match self {
            Sizing::Auto => (cores / 2).max(1),
            Sizing::Fixed(n) => n.max(1),
        }
    }
}

/// Logical cores this process may run on.
pub fn logical_cores() -> u32 {
    std::thread::available_parallelism()
        .map(|n| u32::try_from(n.get()).unwrap_or(u32::MAX))
        .unwrap_or(1)
}

/// The cores a heavy job should count: physical where the OS reports them,
/// never more than the logical count (which also honours a cgroup or affinity
/// limit). Hyper-threads share a core, so 6 physical cores with 12 threads
/// count 6 here; the width and the load test follow from that.
pub fn cores() -> u32 {
    detect_cores().value
}

/// [`cores`] and how it was found ([`host::choose_cores`]). The physical
/// count is read from the OS once per process: a waiter probes every 200 ms.
pub fn detect_cores() -> host::Resolved<u32> {
    static PHYSICAL: std::sync::OnceLock<Option<u32>> = std::sync::OnceLock::new();
    host::choose_cores(*PHYSICAL.get_or_init(physical_cores_os), logical_cores())
}

/// The physical core count the OS reports, if it reports one.
#[cfg(target_os = "macos")]
fn physical_cores_os() -> Option<u32> {
    use std::ffi::{c_char, c_void, CString};
    extern "C" {
        fn sysctlbyname(
            name: *const c_char,
            oldp: *mut c_void,
            oldlenp: *mut usize,
            newp: *mut c_void,
            newlen: usize,
        ) -> i32;
    }
    let name = CString::new("hw.physicalcpu").ok()?;
    let mut value: i32 = 0;
    let mut len = std::mem::size_of::<i32>();
    // SAFETY: `name` is a valid NUL-terminated string; `value` and `len` are
    // a writable i32 and its size, as sysctlbyname's oldp/oldlenp contract wants.
    let rc = unsafe {
        sysctlbyname(
            name.as_ptr(),
            (&mut value as *mut i32).cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    (rc == 0 && value > 0).then(|| u32::try_from(value).unwrap_or(u32::MAX))
}

/// The physical core count from sysfs first (one `core_cpus_list` per CPU),
/// then `/proc/cpuinfo`. Neither reports a count on some hosts (aarch64 VMs),
/// so the caller falls back to the logical count.
#[cfg(target_os = "linux")]
fn physical_cores_os() -> Option<u32> {
    let lists: Vec<String> = fs::read_dir("/sys/devices/system/cpu")
        .ok()?
        .filter_map(Result::ok)
        .filter(|e| {
            let n = e.file_name();
            let n = n.to_string_lossy();
            n.starts_with("cpu") && n[3..].chars().all(|c| c.is_ascii_digit()) && n.len() > 3
        })
        .filter_map(|e| fs::read_to_string(e.path().join("topology/core_cpus_list")).ok())
        .collect();
    host::physical_from_core_lists(lists.iter().map(String::as_str)).or_else(|| {
        fs::read_to_string("/proc/cpuinfo")
            .ok()
            .and_then(|text| host::physical_from_cpuinfo(&text))
    })
}

/// No physical count is read on other platforms: the logical count stands.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn physical_cores_os() -> Option<u32> {
    None
}

/// The machine a sizing is computed for: its core count, passed explicitly so
/// the cap, the width and the admission load test all see the same number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Machine {
    pub cores: u32,
}

impl Machine {
    /// This machine, as [`cores`] reports it.
    pub fn detect() -> Machine {
        Machine { cores: cores() }
    }
}

/// The variables that bound how wide a heavy job runs. A holder that ran at
/// the tool's default width — `cargo` and its test harness use every core —
/// made `cap` holders run `cap × cores` jobs at once. vitest 4 starts
/// `availableParallelism() - 1` workers unless `VITEST_MAX_WORKERS` says
/// otherwise; the variable outranks the project's `maxWorkers` and its
/// `fileParallelism: false`, so a suite that must run its files one at a time
/// sets `VITEST_MAX_WORKERS=1` itself (a set key is kept, see [`width_env`]).
/// `GOMAXPROCS` bounds Go: `go build` and `go test` default `-p` to it, and
/// each test binary inherits it, which caps `-parallel` too.
pub const WIDTH_KEYS: [&str; 6] = [
    "MAKEFLAGS",
    "CARGO_BUILD_JOBS",
    "RUST_TEST_THREADS",
    "NEXTEST_TEST_THREADS",
    "VITEST_MAX_WORKERS",
    "GOMAXPROCS",
];

/// How many jobs a holder of `weight` slots may start:
/// `max(1, weight × cores / cap)`. relais's verification commands use the
/// same formula, so a weight means the same width whoever holds it.
pub fn parallelism(weight: u32, cores: u32, cap: u32) -> u32 {
    let jobs = u64::from(weight) * u64::from(cores) / u64::from(cap.max(1));
    u32::try_from(jobs).unwrap_or(u32::MAX).max(1)
}

/// What to set so a child runs `jobs` wide, given `current`: the value each
/// key would have in the child (its own setting, else the inherited one).
///
/// A key already set to a non-empty value is kept: the check's own choice,
/// or a parent's bound (relais exports these to the commands it runs),
/// wins. An empty value counts as unset — cargo refuses `CARGO_BUILD_JOBS=`.
/// A `MAKEFLAGS` that carries flags but no job bound (`--no-print-directory`,
/// `-s`) gains ` -j{jobs}` rather than staying unbounded.
pub fn width_env(
    jobs: u32,
    current: impl Fn(&str) -> Option<String>,
) -> Vec<(&'static str, String)> {
    let jobs = jobs.max(1);
    WIDTH_KEYS
        .iter()
        .filter_map(|&key| match current(key).filter(|v| !v.trim().is_empty()) {
            None if key == "MAKEFLAGS" => Some((key, format!("-j{jobs}"))),
            None => Some((key, jobs.to_string())),
            Some(v) if key == "MAKEFLAGS" && !bounds_make(&v) => {
                Some((key, format!("{} -j{jobs}", v.trim_end())))
            }
            Some(_) => None,
        })
        .collect()
}

/// Whether a `MAKEFLAGS` value already bounds make's jobs: a `-j`/`--jobs`
/// flag, a jobserver, or make's own short form (`j4` in the first word).
fn bounds_make(flags: &str) -> bool {
    let mut words = flags.split_whitespace();
    let first_is_short = words
        .clone()
        .next()
        .is_some_and(|w| !w.starts_with('-') && !w.contains('=') && w.contains('j'));
    first_is_short
        || words
            .any(|w| w.starts_with("-j") || w.starts_with("--jobs") || w.starts_with("--jobserver"))
}

/// Bound `cmd` to `jobs` wide: [`width_env`] over what the child would see,
/// the command's own `env` first, then this process's environment.
pub fn bound_width(cmd: &mut std::process::Command, jobs: u32) {
    let own: Vec<(String, Option<String>)> = cmd
        .get_envs()
        .map(|(k, v)| {
            (
                k.to_string_lossy().into_owned(),
                v.map(|v| v.to_string_lossy().into_owned()),
            )
        })
        .collect();
    let current = |key: &str| match own.iter().find(|(k, _)| k == key) {
        // `env_remove` on the command: the child sees nothing.
        Some((_, v)) => v.clone(),
        None => std::env::var(key).ok(),
    };
    for (key, value) in width_env(jobs, current) {
        cmd.env(key, value);
    }
}

/// The one-minute load average, where it can be read.
/// Copied from greengate-runtime's `load.rs` (a leaf reading, ADR-0039 allows
/// the copy): a crate with no dependencies cannot reach it.
pub fn load1() -> Option<f64> {
    platform::load1()
}

/// The slot directory: `GREENHARNESS_SLOT_DIR`, else `$XDG_RUNTIME_DIR/greenharness-slots`
/// on Linux, else `/tmp/greenharness-slots-<uid>`. A FIXED path, never
/// `std::env::temp_dir()`, which follows a `$TMPDIR` that macOS and agent
/// sandboxes set per session — every session would get a queue of its own.
pub fn dir() -> PathBuf {
    if let Some(d) = std::env::var_os(DIR_ENV).filter(|d| !d.is_empty()) {
        return PathBuf::from(d);
    }
    #[cfg(target_os = "linux")]
    if let Some(d) = std::env::var_os("XDG_RUNTIME_DIR").filter(|d| !d.is_empty()) {
        return PathBuf::from(d).join("greenharness-slots");
    }
    PathBuf::from(format!("/tmp/greenharness-slots-{}", platform::uid()))
}

/// Make `dir` (and its `queue/` and `holders/`) if needed, then refuse it
/// unless it is a real directory this user owns with mode 0700: on a shared
/// `/tmp`, another user could create it first and hold every slot.
pub fn safe_dir(dir: &Path) -> Result<(), String> {
    platform::make_private(dir)?;
    platform::check_private(dir)?;
    for sub in ["queue", "holders"] {
        platform::make_private(&dir.join(sub))?;
    }
    Ok(())
}

/// An exclusive lock on `path`, retried until `deadline` has passed:
/// `Ok(Some(file))` held until the file drops (and by the kernel if the
/// process dies), `Ok(None)` when another holder kept it past the deadline.
/// On a platform without `flock` (Windows) no lock is taken and the answer
/// is `Ok(Some(file))` at once: callers there run unlocked, as every other
/// writer of the same state does.
pub fn lock_within(path: &Path, deadline: Duration) -> io::Result<Option<std::fs::File>> {
    if !SUPPORTED {
        return std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)
            .map(Some);
    }
    let until = Instant::now() + deadline;
    loop {
        if let Some(file) = platform::try_lock(path)? {
            return Ok(Some(file));
        }
        if Instant::now() >= until {
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Whether slots work on this platform (`flock`): not on Windows.
pub const SUPPORTED: bool = platform::SUPPORTED;

/// Who holds a slot, written to `holders/slot-N.who` while it does.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Holder {
    pub pid: u32,
    /// `amont`, `relais`, `amont slot run`, …
    pub tool: String,
    /// The repository or directory the job runs in; for a push rehearsal,
    /// the worktree its snapshot was taken from.
    pub repo: String,
    /// What it runs, short: `cargo test`, `pre-commit-clippy`.
    pub command: String,
    /// The agent session, when one is known.
    pub session: String,
    pub started_ms: u128,
    /// How many jobs the holder may run ([`parallelism`]); absent in a
    /// record written by an older greengate-slots.
    pub jobs: Option<u32>,
}

impl Holder {
    /// This process, running `command` for `tool`, in the current directory.
    pub fn here(tool: &str, command: &str) -> Holder {
        Holder {
            pid: std::process::id(),
            tool: tool.to_string(),
            repo: std::env::current_dir()
                .map(|d| d.display().to_string())
                .unwrap_or_default(),
            command: command.to_string(),
            session: session_id(),
            started_ms: now_ms(),
            jobs: None,
        }
    }

    /// The session, cut to its first 8 characters: enough to tell sessions
    /// apart and to find the transcript; `-` when none is known.
    pub fn short_session(&self) -> &str {
        match self.session.char_indices().nth(8) {
            _ if self.session.is_empty() => "-",
            Some((i, _)) => &self.session[..i],
            None => &self.session,
        }
    }

    /// `relais: cargo test (greengate-wt-x, 4m)`, for a waiter's line.
    pub fn describe(&self, now_ms: u128) -> String {
        let repo = Path::new(&self.repo)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.repo.clone());
        let mins = now_ms.saturating_sub(self.started_ms) / 60_000;
        format!("{}: {} ({repo}, {mins}m)", self.tool, self.command)
    }

    /// The record as one JSON object: what `holders/slot-N.who` holds, and
    /// what `amont slots --json` lists.
    pub fn to_json(&self) -> String {
        self.to_json_with(None)
    }

    /// [`Holder::to_json`], with `lease` first when given: the record a
    /// [`Lease`] writes. The id stays off `Holder` itself, so its public
    /// shape, its equality and `amont slots --json` do not change.
    fn to_json_with(&self, lease: Option<&str>) -> String {
        let lease = lease.map_or(String::new(), |l| format!("\"lease\":\"{}\",", escape(l)));
        let jobs = self
            .jobs
            .map_or(String::new(), |j| format!(",\"jobs\":{j}"));
        format!(
            "{{{lease}\"pid\":{},\"tool\":\"{}\",\"repo\":\"{}\",\"command\":\"{}\",\"session\":\"{}\",\"started_ms\":{}{jobs}}}",
            self.pid,
            escape(&self.tool),
            escape(&self.repo),
            escape(&self.command),
            escape(&self.session),
            self.started_ms
        )
    }

    fn from_json(s: &str) -> Option<Holder> {
        Some(Holder {
            pid: raw(s, "pid")?.parse().ok()?,
            tool: unescape(&raw(s, "tool")?),
            repo: unescape(&raw(s, "repo")?),
            command: unescape(&raw(s, "command")?),
            session: unescape(&raw(s, "session").unwrap_or_default()),
            started_ms: raw(s, "started_ms")?.parse().ok()?,
            jobs: raw(s, "jobs").and_then(|j| j.parse().ok()),
        })
    }
}

/// The agent session this process runs for: Claude Code's
/// `CLAUDE_CODE_SESSION_ID`, else relais's `RELAIS_SESSION_ID`, else the
/// older `CLAUDE_SESSION_ID`; empty when none is set.
pub fn session_id() -> String {
    session_from(|k| std::env::var(k).ok())
}

const SESSION_ENVS: [&str; 3] = [
    "CLAUDE_CODE_SESSION_ID",
    "RELAIS_SESSION_ID",
    "CLAUDE_SESSION_ID",
];

fn session_from(get: impl Fn(&str) -> Option<String>) -> String {
    SESSION_ENVS
        .iter()
        .find_map(|k| get(k).filter(|v| !v.is_empty()))
        .unwrap_or_default()
}

/// Held slots: released when dropped, and by the kernel if the process dies.
#[derive(Debug)]
pub struct Lease {
    files: Vec<File>,
    who: Vec<PathBuf>,
    /// `<pid>-<seq>`: written into every holder record of this lease, so a
    /// reader of `holders/` tells two leases apart even when their records
    /// are otherwise equal, and sees one lease once while it is renamed.
    id: String,
    /// Boxed: it carries the whole holder, and `Poll::Granted` holds a lease.
    release: Box<Release>,
}

/// What the release row of a [`Lease`] says: who took it, and when.
#[derive(Debug)]
struct Release {
    dir: PathBuf,
    /// The holder at the grant: its tool, session, command and jobs.
    holder: Holder,
    /// Monotonic, for `held_ms`: a wall-clock step never makes it negative.
    granted: Instant,
    /// Wall clock, for pairing the release with its `admitted` row.
    granted_ms: u128,
}

/// Lease ids are unique within a process; the pid makes them unique on the
/// host.
static LEASE_SEQ: AtomicUsize = AtomicUsize::new(0);

/// A holder record as written to `holders/slot-N.who`: the holder's JSON with
/// its lease id first.
fn record_json(holder: &Holder, lease: &str) -> String {
    holder.to_json_with(Some(lease))
}

/// Write `json` to the holder record `path` through a hidden temp file and a
/// rename, so a concurrent reader sees the old record or the new one, never a
/// truncated one. The error names the path that failed.
fn write_record(path: &Path, json: &str) -> io::Result<()> {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return Err(io::Error::other(format!(
            "not a record path: {}",
            path.display()
        )));
    };
    let temp = path.with_file_name(format!(".{file_name}.{}.tmp", std::process::id()));
    fs::write(&temp, json).map_err(|error| {
        let _ = fs::remove_file(&temp); // best-effort: a partly written temp of our own
        io::Error::new(
            error.kind(),
            format!("cannot write {}: {error}", temp.display()),
        )
    })?;
    fs::rename(&temp, path).map_err(|error| {
        let _ = fs::remove_file(&temp); // best-effort: cleanup of our own temp file
        io::Error::new(
            error.kind(),
            format!(
                "cannot rename {} over {}: {error}",
                temp.display(),
                path.display()
            ),
        )
    })
}

impl Lease {
    /// How many slots this lease holds.
    pub fn weight(&self) -> usize {
        self.files.len()
    }

    /// Rewrite the holder record of each slot of this lease as `holder`, with
    /// its start time reset to now, so the age shown is the new holder's. The
    /// record is written to a hidden file in `holders/` and renamed over
    /// `slot-N.who`, so a concurrent reader sees the old record or the new
    /// one, never a truncated one. A `holder` with no `jobs` keeps the width
    /// the record already carries: the lease's width does not change with
    /// its holder. A record that cannot be written stays as it was, and the
    /// error names the path.
    pub fn rename(&self, holder: &Holder) -> io::Result<()> {
        let mut renamed = holder.clone();
        renamed.started_ms = now_ms();
        // Every record is attempted: one that cannot be written must not
        // leave the lease's other records naming the old holder.
        let mut first_error: Option<io::Error> = None;
        for path in &self.who {
            let mut record = renamed.clone();
            if record.jobs.is_none() {
                record.jobs = fs::read_to_string(path)
                    .ok()
                    .and_then(|text| Holder::from_json(&text))
                    .and_then(|old| old.jobs);
            }
            // The lease id comes from the lease, never from the old file: a
            // record that cannot be read must not lose it.
            if let Err(error) = write_record(path, &record_json(&record, &self.id)) {
                first_error.get_or_insert(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }
}

impl Drop for Lease {
    /// The holder record first, then the locks, then the count: a releasing
    /// holder never deletes its successor's record, and the count never reads
    /// zero while this process still holds a flock. Then one `released` row
    /// in `waits.jsonl`, best-effort. A process that exits without unwinding
    /// (`process::exit`, a kill) leaves no such row; the kernel frees its
    /// flocks.
    fn drop(&mut self) {
        for w in &self.who {
            let _ = fs::remove_file(w); // best-effort: a stale record is ignored by pid
        }
        let n = self.files.len();
        self.files.clear();
        HELD.fetch_sub(n, Ordering::SeqCst);
        self.release.record(n);
    }
}

impl Release {
    /// `{"at_ms":…,"tool":…,"pid":…,"session":…,"command":…,"weight":…,
    /// "jobs":…,"granted_ms":…,"held_ms":…,"outcome":"released"}`: one line
    /// per lease, however many gates it was handed across. It has no
    /// `waited_ms`, so a reader of waits selects `has("waited_ms")`.
    fn record(&self, weight: usize) {
        let holder = &self.holder;
        let session = if holder.session.is_empty() {
            String::new()
        } else {
            format!(",\"session\":\"{}\"", escape(&holder.session))
        };
        let jobs = holder
            .jobs
            .map_or(String::new(), |j| format!(",\"jobs\":{j}"));
        let line = format!(
            "{{\"at_ms\":{},\"tool\":\"{}\",\"pid\":{}{session},\"command\":\"{}\",\"weight\":{weight}{jobs},\"granted_ms\":{},\"held_ms\":{},\"outcome\":\"released\"}}\n",
            now_ms(),
            escape(&holder.tool),
            std::process::id(),
            escape(&holder.command),
            self.granted_ms,
            self.granted.elapsed().as_millis(),
        );
        append_journal(&self.dir, &line);
    }
}

/// What a waiter sees while it waits.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Queue {
    /// This waiter's place: 0 is the head.
    pub position: usize,
    /// Waiters in line, this one included.
    pub waiting: usize,
    /// Who holds the slots now, where they said so.
    pub holders: Vec<Holder>,
    /// Slots held by a client that writes no record (amont ≤ 1.61, old
    /// relais).
    pub unknown: usize,
    /// Why this waiter was not admitted on this poll, when it was decided.
    pub reason: Option<WaitReason>,
}

/// Why a waiter was not admitted: what a person reading the queue line can
/// act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitReason {
    /// Others are ahead of it in the queue.
    Behind { ahead: usize },
    /// Every slot is held.
    SlotsFull { held: usize, cap: u32 },
    /// A slot is free, but the one-minute load is at or above the limit,
    /// `cores + LOAD_PER_JOB × holder_jobs` (ADR-0022). The load is in tenths
    /// (`310` is 31.0), so the queue stays `Eq`.
    Load {
        load_tenths: u32,
        cores: u32,
        /// The jobs of the holders, which raise the limit.
        holder_jobs: u32,
    },
    /// A slot is free, but a holder was admitted under a minute ago.
    Ramp,
}

impl WaitReason {
    /// `load 31.0 >= 14 (6 cores + 4×2 holder jobs)`, `slots full 3/3`,
    /// `2 ahead`, `ramp`.
    pub fn describe(&self) -> String {
        match self {
            WaitReason::Behind { ahead } => format!("{ahead} ahead"),
            WaitReason::SlotsFull { held, cap } => format!("slots full {held}/{cap}"),
            WaitReason::Load {
                load_tenths,
                cores,
                holder_jobs,
            } => format!(
                "load {}.{} >= {} ({cores} cores + {LOAD_PER_JOB}×{holder_jobs} holder jobs)",
                load_tenths / 10,
                load_tenths % 10,
                load_limit(*cores, *holder_jobs)
            ),
            WaitReason::Ramp => "one new holder per minute".to_string(),
        }
    }
}

/// The answer to one [`Waiter::poll`].
#[derive(Debug)]
pub enum Poll {
    Granted(Lease),
    Waiting(Queue),
}

/// What the world looks like to one admission decision: the seam the tests
/// drive instead of the real load and clock.
#[derive(Debug, Clone, Copy)]
pub struct Probe {
    pub load1: Option<f64>,
    pub cores: u32,
    pub now_ms: u128,
    /// Slots the asking process already holds ([`held_in_process`]).
    pub own_held: usize,
}

impl Probe {
    pub fn now() -> Probe {
        Probe {
            load1: load1(),
            cores: cores(),
            now_ms: now_ms(),
            own_held: held_in_process(),
        }
    }
}

/// A place in the queue, held across polls. Dropping it leaves the queue.
#[derive(Debug)]
pub struct Waiter {
    dir: PathBuf,
    sizing: Sizing,
    machine: Machine,
    weight: u32,
    holder: Holder,
    ticket: Option<(PathBuf, File)>,
}

impl Waiter {
    /// Join the queue in [`dir`].
    pub fn new(sizing: Sizing, weight: u32, holder: Holder) -> io::Result<Waiter> {
        Waiter::new_in(&dir(), sizing, weight, holder)
    }

    /// Join the queue in `dir`. The ticket is created under a hidden name,
    /// locked, and only then renamed into the queue — a rename keeps the
    /// lock, so no visible ticket is ever unlocked, and cleanup (which removes
    /// visible tickets whose lock is free) can never take a live one.
    pub fn new_in(dir: &Path, sizing: Sizing, weight: u32, holder: Holder) -> io::Result<Waiter> {
        Waiter::new_in_on(dir, sizing, weight, holder, Machine::detect())
    }

    /// [`Waiter::new_in`] for a given machine: the width is computed from
    /// `machine.cores` and the sizing's cap on it.
    pub fn new_in_on(
        dir: &Path,
        sizing: Sizing,
        weight: u32,
        holder: Holder,
        machine: Machine,
    ) -> io::Result<Waiter> {
        safe_dir(dir).map_err(io::Error::other)?;
        let queue = dir.join("queue");
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        let seq = SEQ.fetch_add(1, Ordering::SeqCst);
        let pid = std::process::id();
        let hidden = queue.join(format!(".new-{pid}-{seq}"));
        let file = platform::try_lock(&hidden)?
            .ok_or_else(|| io::Error::other("a fresh ticket was already locked"))?;
        let visible = queue.join(format!("{nanos:024}-{pid}-{seq}"));
        fs::rename(&hidden, &visible)?;
        let weight = weight.max(1);
        let mut holder = holder;
        if holder.jobs.is_none() {
            holder.jobs = Some(parallelism(
                weight,
                machine.cores,
                sizing.cap_for(machine.cores),
            ));
        }
        Ok(Waiter {
            dir: dir.to_path_buf(),
            sizing,
            machine,
            weight,
            holder,
            ticket: Some((visible, file)),
        })
    }

    /// Ask once, without blocking.
    pub fn poll(&mut self) -> io::Result<Poll> {
        self.poll_with(self.probe())
    }

    /// The probe [`Waiter::poll`] decides with: the load and clock now, and
    /// the cores of the machine this waiter was sized for, so an override of
    /// the core count reaches the load test as well as the width.
    pub(crate) fn probe(&self) -> Probe {
        Probe {
            cores: self.machine.cores,
            ..Probe::now()
        }
    }

    /// [`Waiter::poll`] against a given load and clock.
    pub fn poll_with(&mut self, probe: Probe) -> io::Result<Poll> {
        // The admission lock, without blocking: a stopped or stuck holder of
        // it must never hold a hook past its own deadline.
        let Some(_admission) = platform::try_lock(&self.dir.join("admission"))? else {
            return Ok(Poll::Waiting(Queue::default()));
        };
        let mine = self.ticket.as_ref().map(|(p, _)| p.clone());
        let live = live_tickets(&self.dir, mine.as_deref())?;
        let position = mine
            .as_ref()
            .and_then(|m| live.iter().position(|t| t == m))
            .unwrap_or(0);
        let cap = self.sizing.cap_for(probe.cores);
        let Probed { free, held } = probe_slots(&self.dir, cap)?;
        let weight = self.weight.min(cap) as usize;
        // The holders' jobs raise the load limit (ADR-0022): their own load
        // must not keep the next slot shut. Read only when a load test runs.
        let holder_jobs = if held.is_empty() || matches!(self.sizing, Sizing::Fixed(_)) {
            0
        } else {
            held_jobs(&self.dir, &held, parallelism(1, probe.cores, cap))
        };
        let limit = f64::from(load_limit(probe.cores, holder_jobs));
        let admit = position == 0 && free.len() >= weight && {
            let floor = held.is_empty();
            match self.sizing {
                Sizing::Fixed(_) => true,
                Sizing::Auto => {
                    floor || {
                        let below = probe.load1.map_or(true, |l| l < limit);
                        let since = probe.now_ms.saturating_sub(admitted_at(&self.dir));
                        // A sibling of a holder in this process (one commit's
                        // own checks) skips the ramp: the ramp spaces
                        // separate jobs, and spacing one commit's checks only
                        // made it slower (10m48s observed). The load test and
                        // the cap still apply.
                        let ramped = since >= RAMP.as_millis() || probe.own_held > 0;
                        below && ramped
                    }
                }
            }
        };
        if !admit {
            let (holders, unknown) = read_holders(&self.dir, &held);
            let reason = if position > 0 {
                WaitReason::Behind { ahead: position }
            } else if free.len() < weight {
                WaitReason::SlotsFull {
                    held: held.len(),
                    cap,
                }
            } else {
                match probe.load1 {
                    Some(load1) if load1 >= limit => WaitReason::Load {
                        // In tenths, without a lossy cast: a load is a small
                        // non-negative number, and anything else saturates.
                        load_tenths: format!("{:.0}", load1 * 10.0).parse().unwrap_or(u32::MAX),
                        cores: probe.cores,
                        holder_jobs,
                    },
                    _ => WaitReason::Ramp,
                }
            };
            return Ok(Poll::Waiting(Queue {
                position,
                waiting: live.len(),
                holders,
                unknown,
                reason: Some(reason),
            }));
        }
        let id = format!(
            "{}-{}",
            std::process::id(),
            LEASE_SEQ.fetch_add(1, Ordering::Relaxed)
        );
        let record = record_json(&self.holder, &id);
        let mut files = Vec::new();
        let mut who = Vec::new();
        for (index, file) in free.into_iter().take(weight) {
            let path = self.dir.join("holders").join(format!("slot-{index}.who"));
            let _ = write_record(&path, &record); // best-effort: a missing record shows "unknown"
            who.push(path);
            files.push(file);
        }
        HELD.fetch_add(files.len(), Ordering::SeqCst);
        let _ = fs::write(self.dir.join("admitted-at"), probe.now_ms.to_string()); // best-effort: a lost stamp only shortens one ramp
        self.leave();
        remove_dead_holder_files(&self.dir);
        // The real clocks, never the probe's: tests fake `probe.now_ms`.
        let release = Box::new(Release {
            dir: self.dir.clone(),
            holder: self.holder.clone(),
            granted: Instant::now(),
            granted_ms: now_ms(),
        });
        Ok(Poll::Granted(Lease {
            files,
            who,
            id,
            release,
        }))
    }

    fn leave(&mut self) {
        if let Some((path, file)) = self.ticket.take() {
            let _ = fs::remove_file(&path); // best-effort: cleanup removes an unlocked ticket
            drop(file);
        }
    }
}

impl Drop for Waiter {
    fn drop(&mut self) {
        self.leave();
    }
}

/// Wait for `weight` slots while the line moves, at most `budget` (ADR-0022:
/// [`wait_in_line`] with the [`STALL`] window). `Ok(None)` when the wait
/// ended without a slot, `Err` when polling kept failing. `on_wait` sees
/// each answer while waiting.
pub fn acquire(
    sizing: Sizing,
    weight: u32,
    holder: Holder,
    budget: Duration,
    on_wait: impl FnMut(&Queue),
) -> io::Result<Option<Lease>> {
    let waiter = Waiter::new(sizing, weight, holder)?;
    let bounds = WaitBounds {
        window: Some(STALL),
        ceiling: budget,
    };
    match wait_in_line(waiter, bounds, on_wait) {
        Ok(lease) => Ok(Some(lease)),
        Err(LineEnd::PollFailed { error }) => Err(error),
        Err(LineEnd::Stalled { .. } | LineEnd::Ceiling { .. }) => Ok(None),
    }
}

/// How a wait in line is bounded (ADR-0022): the stall window (`None`: no
/// stall rule) and the caller's hard ceiling. A window as long as the
/// ceiling would never end a wait first, so it is not applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaitBounds {
    pub window: Option<Duration>,
    pub ceiling: Duration,
}

/// Why a wait in line ended without a slot.
#[derive(Debug)]
pub enum LineEnd {
    /// No admission anywhere and no change of place for `window`.
    Stalled { window: Duration, position: usize },
    /// The caller's hard ceiling ran out after `waited`.
    Ceiling { waited: Duration },
    /// [`POLL_FAILURES`] polls in a row failed after the queue was joined.
    PollFailed { error: io::Error },
}

impl LineEnd {
    /// The journal's `"end"` for this ending.
    pub fn end(&self) -> WaitEnd {
        match self {
            LineEnd::Stalled { .. } => WaitEnd::Stalled,
            LineEnd::Ceiling { .. } => WaitEnd::Ceiling,
            LineEnd::PollFailed { .. } => WaitEnd::IoError,
        }
    }
}

/// How many polls in a row may fail before a wait ends: one failed read of
/// the slot files (a busy disk, a record mid-rename) is not a broken queue.
pub const POLL_FAILURES: u32 = 3;

/// Wait in line until a slot is granted, the line stalls, or the ceiling
/// runs out (ADR-0022): the one loop every caller uses, so greengate's hooks,
/// `slot run` and relais end a wait the same way. It takes the waiter: when
/// it returns, granted or not, this caller has left the queue. `on_wait` sees
/// each answer while waiting.
pub fn wait_in_line(
    mut waiter: Waiter,
    bounds: WaitBounds,
    on_wait: impl FnMut(&Queue),
) -> Result<Lease, LineEnd> {
    let joined = Instant::now();
    let dir = waiter.dir.clone();
    wait_in_line_with(
        bounds,
        || waiter.poll(),
        || (joined.elapsed(), now_ms(), admitted_at(&dir)),
        std::thread::sleep,
        on_wait,
    )
}

/// [`wait_in_line`] against given polls and clocks: `clock` answers the time
/// since the wait began (monotonic), then now and the last admission (wall
/// milliseconds). The seam the tests drive.
fn wait_in_line_with(
    bounds: WaitBounds,
    mut poll: impl FnMut() -> io::Result<Poll>,
    mut clock: impl FnMut() -> (Duration, u128, u128),
    mut sleep: impl FnMut(Duration),
    mut on_wait: impl FnMut(&Queue),
) -> Result<Lease, LineEnd> {
    let mut stall = bounds
        .window
        .filter(|window| *window < bounds.ceiling)
        .map(Stall::new);
    let mut failures = 0;
    loop {
        match poll() {
            Ok(Poll::Granted(lease)) => return Ok(lease),
            Ok(Poll::Waiting(queue)) => {
                failures = 0;
                on_wait(&queue);
                let (elapsed, now, admitted) = clock();
                if let Some(stall) = stall.as_mut() {
                    if stall.observe_at(&queue, elapsed, now, admitted) {
                        return Err(LineEnd::Stalled {
                            window: stall.limit(),
                            position: queue.position,
                        });
                    }
                }
            }
            Err(error) => {
                failures += 1;
                if failures >= POLL_FAILURES {
                    return Err(LineEnd::PollFailed { error });
                }
            }
        }
        let (elapsed, _, _) = clock();
        if elapsed >= bounds.ceiling {
            return Err(LineEnd::Ceiling { waited: elapsed });
        }
        sleep(Duration::from_millis(200));
    }
}

/// The load limit a further slot is taken under: `cores + LOAD_PER_JOB ×
/// holder_jobs`, saturating (ADR-0022).
pub fn load_limit(cores: u32, holder_jobs: u32) -> u32 {
    cores.saturating_add(LOAD_PER_JOB.saturating_mul(holder_jobs))
}

/// Whether a wait's line has stopped moving (ADR-0022): one rule, so every
/// caller (greengate's hooks, `slot run`, relais) ends a wait the same way.
///
/// The idle time is the shorter of two: the time since this waiter joined or
/// last changed place, on a monotonic clock, so a laptop's sleep or a clock
/// step cannot make a moving line look stalled; and the time since the last
/// admission anywhere, from the machine-wide `admitted-at` stamp, which other
/// processes write on the wall clock. A stamp ahead of now counts as just now,
/// never as a panic or an instant stall. A poll that missed the admission lock
/// (`Queue::reason` is `None`) moves no mark: it saw nothing.
#[derive(Debug, Clone)]
pub struct Stall {
    window: Duration,
    joined: Instant,
    /// When this waiter last changed place, as time since `joined`.
    moved: Duration,
    position: Option<usize>,
}

impl Stall {
    /// A clock for a wait that starts now, ending after `window` without
    /// progress.
    pub fn new(window: Duration) -> Stall {
        Stall {
            window,
            joined: Instant::now(),
            moved: Duration::ZERO,
            position: None,
        }
    }

    /// The window to stall after, from the caller's reading of
    /// [`STALL_ENV`] (whole seconds from 1): [`STALL`] when unset. An empty,
    /// zero or unreadable value gives [`STALL`] and a warning for the caller
    /// to print once. Each binary reads the variable at its edge; the crate
    /// reads no environment for it.
    pub fn window_from(setting: Option<&str>) -> (Duration, Option<String>) {
        match setting {
            None => (STALL, None),
            Some(text) => match text.trim().parse::<u64>() {
                Ok(secs) if secs >= 1 => (Duration::from_secs(secs), None),
                _ => (
                    STALL,
                    Some(format!(
                        "{STALL_ENV}={text:?} is not a whole number of seconds from 1; using {}",
                        STALL.as_secs()
                    )),
                ),
            },
        }
    }

    /// Feed one poll's answer, at the real clocks: whether the line has now
    /// made no progress for the window.
    pub fn observe(&mut self, waiter: &Waiter, queue: &Queue) -> bool {
        self.observe_at(
            queue,
            self.joined.elapsed(),
            now_ms(),
            admitted_at(&waiter.dir),
        )
    }

    /// [`Stall::observe`] against given clocks: `elapsed` since the wait
    /// began (monotonic), `now_ms` and the last admission `admitted_ms` (wall).
    pub fn observe_at(
        &mut self,
        queue: &Queue,
        elapsed: Duration,
        now_ms: u128,
        admitted_ms: u128,
    ) -> bool {
        if queue.reason.is_some() {
            if self.position.is_some_and(|p| p != queue.position) {
                self.moved = elapsed;
            }
            self.position = Some(queue.position);
        }
        self.idle_at(elapsed, now_ms, admitted_ms) >= self.window
    }

    /// How long the line has made no progress, at the given clocks.
    pub fn idle_at(&self, elapsed: Duration, now_ms: u128, admitted_ms: u128) -> Duration {
        let own = elapsed.saturating_sub(self.moved);
        let since_admission = u64::try_from(now_ms.saturating_sub(admitted_ms))
            .map_or(Duration::MAX, Duration::from_millis);
        own.min(since_admission)
    }

    /// The window this clock stalls after.
    pub fn limit(&self) -> Duration {
        self.window
    }
}

/// What tells one lease's records apart: its `"lease"` id, or the whole
/// record for an older writer that names none.
fn lease_key(text: &str, holder: &Holder) -> String {
    raw(text, "lease").unwrap_or_else(|| holder.to_json())
}

/// The jobs one lease adds to Σjobs: its `jobs`, or `missing` for a record
/// that names none (an older writer). The one count both the load test and
/// the journal's `holder_jobs` use; the caller counts each lease once.
fn lease_jobs(holder: &Holder, missing: u32) -> u32 {
    holder.jobs.unwrap_or(missing)
}

/// Σjobs over the held slots `held`, for the load test: a held slot with no
/// live record (an old client, a record being renamed) counts `missing`.
fn held_jobs(dir: &Path, held: &[u32], missing: u32) -> u32 {
    let mut leases = std::collections::BTreeSet::new();
    let mut jobs = 0u32;
    for index in held {
        let path = dir.join("holders").join(format!("slot-{index}.who"));
        let text = fs::read_to_string(&path).ok();
        let add = match text
            .as_deref()
            .and_then(|t| Holder::from_json(t).map(|h| (t, h)))
        {
            Some((text, holder)) if platform::alive(holder.pid) => {
                if leases.insert(lease_key(text, &holder)) {
                    lease_jobs(&holder, missing)
                } else {
                    0
                }
            }
            _ => missing,
        };
        jobs = jobs.saturating_add(add);
    }
    jobs
}

/// The host's slots right now, for `amont slots`.
#[derive(Debug, Clone, Default)]
pub struct Status {
    pub cap: u32,
    pub holders: Vec<Holder>,
    pub unknown: usize,
    pub waiting: usize,
}

/// Read the slots without taking any: under the admission lock when it is
/// free (so slot probes cannot race an admission), else from the records
/// alone.
pub fn status(sizing: Sizing) -> io::Result<Status> {
    status_in(&dir(), sizing)
}

pub fn status_in(dir: &Path, sizing: Sizing) -> io::Result<Status> {
    status_in_on(dir, sizing, Machine::detect())
}

/// [`status_in`] for a given machine: the cap is the sizing's on its cores.
pub fn status_in_on(dir: &Path, sizing: Sizing, machine: Machine) -> io::Result<Status> {
    safe_dir(dir).map_err(io::Error::other)?;
    let cap = sizing.cap_for(machine.cores);
    let waiting = live_tickets_readonly(dir)?;
    let Some(_admission) = platform::try_lock(&dir.join("admission"))? else {
        let held: Vec<u32> = (0..cap).collect();
        let (holders, _) = read_holders(dir, &held);
        return Ok(Status {
            cap,
            holders,
            unknown: 0,
            waiting,
        });
    };
    let Probed { free, held } = probe_slots(dir, cap)?;
    drop(free);
    let (holders, unknown) = read_holders(dir, &held);
    Ok(Status {
        cap,
        holders,
        unknown,
        waiting,
    })
}

/// Append one acquisition to `waits.jsonl`: the source of the wait
/// percentiles the plan measures. Best-effort.
pub fn record_wait(dir: &Path, tool: &str, weight: u32, waited_ms: u128, outcome: &str) {
    record_wait_jobs(dir, tool, weight, None, waited_ms, outcome);
}

/// [`record_wait`] with the width the holder was given, when known: the
/// line gains `"jobs"`. A separate function so callers of `record_wait`
/// built against an older greengate-slots keep compiling.
pub fn record_wait_jobs(
    dir: &Path,
    tool: &str,
    weight: u32,
    jobs: Option<u32>,
    waited_ms: u128,
    outcome: &str,
) {
    record_wait_named(
        dir,
        tool,
        WaitContext::default(),
        weight,
        jobs,
        waited_ms,
        outcome,
    );
}

/// What the runtime knows about where an acquisition happened.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WaitContext<'a> {
    /// The check that waited, when the caller knows it.
    pub name: Option<&'a str>,
    /// Where the acquisition happened.
    pub place: WaitPlace,
    /// The slot cap the waiter was sized against, when the caller knows it;
    /// written as `"cap"` so lines from before and after a sizing change can
    /// be told apart.
    pub cap: Option<u32>,
    /// The core count the waiter was sized for, when the caller knows it;
    /// written as `"cores"`. `None` writes the detected count.
    pub cores: Option<u32>,
    /// Why a wait that did not admit ended, written as `"end"` (ADR-0022).
    pub end: Option<WaitEnd>,
}

/// Why a wait ended without a slot: the line stalled, the caller's hard
/// ceiling ran out, or the slot files failed after the queue was joined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum WaitEnd {
    Stalled,
    Ceiling,
    IoError,
}

impl WaitEnd {
    /// The journal word: `stalled`, `ceiling`, `io_error`.
    pub fn as_str(self) -> &'static str {
        match self {
            WaitEnd::Stalled => "stalled",
            WaitEnd::Ceiling => "ceiling",
            WaitEnd::IoError => "io_error",
        }
    }
}

/// Where an acquisition happened: a plain push, a push rehearsal in a
/// snapshot, or land's pre-stage rehearsal.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WaitPlace {
    #[default]
    Push,
    Rehearsal,
    Prestage,
}

/// [`record_wait_jobs`] naming the check that waited: the line gains `"name"`
/// (when `context` has one), `"pid"`, `"session"` (when known), and `"rehearsal"` and
/// `"prestage"` when `context` sets them.
///
/// Every line also carries `"holders"` and `"holder_jobs"`: the slots held,
/// and the jobs of the distinct leases holding them, when the line is
/// written ([`occupancy`]). Callers write the line after the grant, so an
/// `admitted` or `handed` line counts its own lease (its `weight` and `jobs`
/// are in the line to subtract), while a `timeout`, `unqueued` or `deferred`
/// line counts only the others. Both are `null` when the holder records
/// could not be read.
pub fn record_wait_named(
    dir: &Path,
    tool: &str,
    context: WaitContext,
    weight: u32,
    jobs: Option<u32>,
    waited_ms: u128,
    outcome: &str,
) {
    let load = load1().map_or("null".to_string(), |l| format!("{l:.2}"));
    let jobs = jobs.map_or(String::new(), |j| format!(",\"jobs\":{j}"));
    let cap = context
        .cap
        .map_or(String::new(), |c| format!(",\"cap\":{c}"));
    let name = context
        .name
        .map_or(String::new(), |n| format!(",\"name\":\"{}\"", escape(n)));
    let session = session_id();
    let session = if session.is_empty() {
        String::new()
    } else {
        format!(",\"session\":\"{}\"", escape(&session))
    };
    let (rehearsal, prestage) = match context.place {
        WaitPlace::Push => ("", ""),
        WaitPlace::Rehearsal => (",\"rehearsal\":true", ""),
        WaitPlace::Prestage => (",\"rehearsal\":true", ",\"prestage\":true"),
    };
    let line_cores = context.cores.unwrap_or_else(cores);
    // A record naming no jobs counts as one slot's width, as the load test
    // counts it, so `holder_jobs` is the Σjobs the admission decided with.
    let missing = parallelism(
        1,
        line_cores,
        context
            .cap
            .unwrap_or_else(|| Sizing::Auto.cap_for(line_cores)),
    );
    let held = occupancy(dir, missing).map_or(
        ",\"holders\":null,\"holder_jobs\":null".to_string(),
        |(slots, jobs)| format!(",\"holders\":{slots},\"holder_jobs\":{jobs}"),
    );
    let end = context
        .end
        .map_or(String::new(), |e| format!(",\"end\":\"{}\"", e.as_str()));
    let line = format!(
        "{{\"at_ms\":{},\"tool\":\"{}\"{name},\"pid\":{}{session},\"weight\":{weight}{jobs}{cap},\"cores\":{line_cores}{held},\"waited_ms\":{waited_ms},\"load1\":{load},\"outcome\":\"{}\"{end}{rehearsal}{prestage}}}\n",
        now_ms(),
        escape(tool),
        std::process::id(),
        escape(outcome)
    );
    append_journal(dir, &line);
}

/// Append one line to `waits.jsonl` in a single write. Best-effort.
// holds-until: t08 finds `waits.jsonl` past 3x its 232-lines-per-12-hours
// baseline; then rotation becomes its own ticket (the t09 plan's Non-goals).
fn append_journal(dir: &Path, line: &str) {
    if let Ok(mut f) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("waits.jsonl"))
    {
        let _ = f.write_all(line.as_bytes()); // best-effort: a lost line costs one sample
    } // else best-effort: an unopenable log costs one sample, never a slot
}

/// The slots held now and the jobs of the distinct leases holding them, from
/// the live `holders/slot-N.who` records alone: no lock is taken, so this is
/// an observation, not an atomic count. A record whose writer is dead is
/// skipped, as is one removed while it was read (a release). Leases are told
/// apart by their `"lease"` id; a record without one (an older writer) is
/// deduplicated by equality, and one naming no `jobs` counts `missing`
/// ([`lease_jobs`], the load test's count). A slot held by a client that
/// writes no record is not seen. `None` when the directory or a record could
/// not be read or parsed: unknown, never zero.
fn occupancy(dir: &Path, missing: u32) -> Option<(usize, u32)> {
    let entries = match fs::read_dir(dir.join("holders")) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Some((0, 0)),
        Err(_) => return None,
    };
    let mut slots = 0;
    let mut jobs = 0u32;
    let mut leases = std::collections::BTreeSet::new();
    for entry in entries {
        let name = entry.ok()?.file_name().to_string_lossy().into_owned();
        if !(name.starts_with("slot-") && name.ends_with(".who")) {
            continue;
        }
        let text = match fs::read_to_string(dir.join("holders").join(&name)) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(_) => return None,
        };
        let holder = Holder::from_json(&text)?;
        if !platform::alive(holder.pid) {
            continue;
        }
        slots += 1;
        if leases.insert(lease_key(&text, &holder)) {
            jobs = jobs.saturating_add(lease_jobs(&holder, missing));
        }
    }
    Some((slots, jobs))
}

/// Milliseconds since the epoch.
pub fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn admitted_at(dir: &Path) -> u128 {
    fs::read_to_string(dir.join("admitted-at"))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

/// Visible tickets in order, removing the ones whose waiter died (their
/// lock is free). `mine` is never probed: this process holds it.
fn live_tickets(dir: &Path, mine: Option<&Path>) -> io::Result<Vec<PathBuf>> {
    let mut live = Vec::new();
    for entry in fs::read_dir(dir.join("queue"))? {
        let path = entry?.path();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name.starts_with('.') {
            continue;
        }
        if Some(path.as_path()) == mine {
            live.push(path);
            continue;
        }
        match platform::try_lock(&path) {
            Ok(Some(_dead)) => {
                let _ = fs::remove_file(&path); // best-effort: a dead waiter's ticket
            }
            _ => live.push(path),
        }
    }
    live.sort();
    Ok(live)
}

/// Remove what a dead process left in `holders/`, as dead queue tickets are
/// removed: the hidden `.slot-N.who.<pid>.tmp` a write leaves when its
/// process dies between the write and the rename, and the `slot-N.who` record
/// of a holder that exited without dropping its lease (a kill, or
/// `process::exit` on a signal). Such a record is told by its slot's flock
/// being free, not by its pid: a reused pid, or one from another pid
/// namespace, says nothing about the slot, and the free flock proves the
/// holder is gone. Left in place, it would count as a holder again. Called
/// under the admission lock, so no grant writes a record meanwhile, and a
/// lease renames only the slots it holds.
fn remove_dead_holder_files(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir.join("holders")) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(slot) = name
            .strip_prefix("slot-")
            .and_then(|rest| rest.strip_suffix(".who"))
            .filter(|index| index.parse::<u32>().is_ok())
        {
            let lock = dir.join(format!("slot-{slot}"));
            // Only a slot file that exists is probed: probing creates it.
            if lock.exists() && matches!(platform::try_lock(&lock), Ok(Some(_))) {
                let _ = fs::remove_file(entry.path()); // best-effort: a gone holder's record
            }
            continue;
        }
        let Some(stem) = name
            .strip_prefix('.')
            .and_then(|rest| rest.strip_suffix(".tmp"))
        else {
            continue;
        };
        let Some(pid) = stem
            .rsplit_once('.')
            .and_then(|(_, pid)| pid.parse::<u32>().ok())
        else {
            continue;
        };
        if !platform::alive(pid) {
            let _ = fs::remove_file(entry.path()); // best-effort: a dead process's temp record
        }
    }
}

/// How many visible tickets there are, without touching any.
fn live_tickets_readonly(dir: &Path) -> io::Result<usize> {
    Ok(fs::read_dir(dir.join("queue"))?
        .filter_map(Result::ok)
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .count())
}

/// The slots below the cap, as one probe found them.
struct Probed {
    /// The free ones, locked by the probe: the caller keeps or drops them.
    free: Vec<(u32, File)>,
    /// The indices of the held ones.
    held: Vec<u32>,
}

/// Lock every free slot below `cap`, and list the held ones. Called only
/// under the admission lock.
fn probe_slots(dir: &Path, cap: u32) -> io::Result<Probed> {
    let mut probed = Probed {
        free: Vec::new(),
        held: Vec::new(),
    };
    for index in 0..cap {
        match platform::try_lock(&dir.join(format!("slot-{index}")))? {
            Some(file) => probed.free.push((index, file)),
            None => probed.held.push(index),
        }
    }
    Ok(probed)
}

/// The records of the held slots whose writer is still alive; a held slot
/// with none is an old client's.
fn read_holders(dir: &Path, held: &[u32]) -> (Vec<Holder>, usize) {
    let mut holders = Vec::new();
    let mut unknown = 0;
    for index in held {
        let path = dir.join("holders").join(format!("slot-{index}.who"));
        match fs::read_to_string(&path)
            .ok()
            .and_then(|s| Holder::from_json(&s))
        {
            Some(h) if platform::alive(h.pid) => {
                if !holders.contains(&h) {
                    holders.push(h);
                }
            }
            _ => unknown += 1,
        }
    }
    (holders, unknown)
}

/// Escape `s` per the JSON spec. Copied from greengate-quiet (a leaf utility,
/// ADR-0039 allows the copy).
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    out.push(ch);
                }
            }
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

/// The raw value of top-level `key` in the flat objects this crate writes: a
/// number as written, a string without its quotes, escapes left in place.
fn raw(s: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\":");
    let rest = &s[s.find(&pat)? + pat.len()..];
    if let Some(body) = rest.strip_prefix('"') {
        let mut end = 0;
        let bytes = body.as_bytes();
        while end < bytes.len() {
            match bytes[end] {
                b'\\' => end += 2,
                b'"' => return Some(body[..end].to_string()),
                _ => end += 1,
            }
        }
        None
    } else {
        let end = rest.find([',', '}']).unwrap_or(rest.len());
        Some(rest[..end].trim().to_string())
    }
}

#[cfg(unix)]
mod platform {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    use std::os::unix::io::AsRawFd;
    use std::path::Path;

    pub const SUPPORTED: bool = true;

    extern "C" {
        #[link_name = "flock"]
        fn libc_flock(fd: i32, op: i32) -> i32;
        #[link_name = "getuid"]
        fn libc_getuid() -> u32;
        #[link_name = "kill"]
        fn libc_kill(pid: i32, sig: i32) -> i32;
    }
    const LOCK_EX: i32 = 2;
    const LOCK_NB: i32 = 4;

    pub fn uid() -> u32 {
        // SAFETY: getuid takes nothing and cannot fail.
        unsafe { libc_getuid() }
    }

    pub fn alive(pid: u32) -> bool {
        let Ok(pid) = i32::try_from(pid) else {
            return false;
        };
        if pid <= 0 {
            return false;
        }
        // SAFETY: signal 0 sends nothing; it only asks whether the pid exists.
        if unsafe { libc_kill(pid, 0) } == 0 {
            return true;
        }
        const ESRCH: i32 = 3;
        std::io::Error::last_os_error().raw_os_error() != Some(ESRCH)
    }

    pub fn make_private(dir: &Path) -> Result<(), String> {
        if dir.symlink_metadata().is_ok() {
            return Ok(());
        }
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))
    }

    pub fn check_private(dir: &Path) -> Result<(), String> {
        let meta = dir
            .symlink_metadata()
            .map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
        if meta.file_type().is_symlink() {
            return Err(format!("{} is a symbolic link", dir.display()));
        }
        if !meta.is_dir() {
            return Err(format!("{} is not a directory", dir.display()));
        }
        if meta.uid() != uid() {
            return Err(format!("{} belongs to another user", dir.display()));
        }
        let mode = meta.permissions().mode() & 0o777;
        if mode != 0o700 {
            return Err(format!("{} has mode {mode:o}, not 700", dir.display()));
        }
        Ok(())
    }

    /// `Ok(None)` only when another holder has the lock (EWOULDBLOCK);
    /// every other failure is an error the caller reports. The file is
    /// opened close-on-exec (std's default), so no child inherits the lock.
    pub fn try_lock(path: &Path) -> std::io::Result<Option<std::fs::File>> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)?;
        // SAFETY: a valid fd we own; flock touches no memory.
        if unsafe { libc_flock(file.as_raw_fd(), LOCK_EX | LOCK_NB) } == 0 {
            return Ok(Some(file));
        }
        let e = std::io::Error::last_os_error();
        if e.kind() == std::io::ErrorKind::WouldBlock {
            Ok(None)
        } else {
            Err(e)
        }
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub fn load1() -> Option<f64> {
        extern "C" {
            #[link_name = "getloadavg"]
            fn getloadavg_raw(loadavg: *mut f64, nelem: i32) -> i32;
        }
        let mut avg = [0.0f64; 3];
        // SAFETY: a valid buffer of three f64; getloadavg writes at most three.
        let n = unsafe { getloadavg_raw(avg.as_mut_ptr(), 3) };
        (n >= 1).then_some(avg[0])
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    pub fn load1() -> Option<f64> {
        None
    }
}

#[cfg(not(unix))]
mod platform {
    use std::path::Path;

    pub const SUPPORTED: bool = false;

    pub fn uid() -> u32 {
        0
    }
    pub fn alive(_pid: u32) -> bool {
        true
    }
    pub fn make_private(_dir: &Path) -> Result<(), String> {
        Ok(())
    }
    pub fn check_private(_dir: &Path) -> Result<(), String> {
        Ok(())
    }
    pub fn try_lock(_path: &Path) -> std::io::Result<Option<std::fs::File>> {
        Ok(None)
    }
    pub fn load1() -> Option<f64> {
        None
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let scratch_dir =
            std::env::temp_dir().join(format!("greengate-slots-{}-{tag}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&scratch_dir); // best-effort: a fresh scratch dir
        scratch_dir
    }

    /// An idle host, asked by a process that holds no slot: separate
    /// processes, though the tests run in one.
    fn idle(now_ms: u128) -> Probe {
        Probe {
            load1: Some(1.0),
            cores: 12,
            now_ms,
            own_held: 0,
        }
    }

    fn busy(now_ms: u128) -> Probe {
        Probe {
            load1: Some(20.0),
            cores: 12,
            now_ms,
            own_held: 0,
        }
    }

    fn granted(p: Poll) -> Lease {
        match p {
            Poll::Granted(l) => l,
            Poll::Waiting(q) => panic!("expected a grant, waiting at {q:?}"),
        }
    }

    fn waiting(p: Poll) -> Queue {
        match p {
            Poll::Waiting(q) => q,
            Poll::Granted(_) => panic!("expected to wait"),
        }
    }

    /// An empty host admits the first waiter whatever the load (the floor).
    #[test]
    fn the_floor_admits_the_first_waiter_even_when_busy() {
        let d = scratch("floor");
        let mut w = Waiter::new_in(&d, Sizing::Fixed(2), 1, Holder::here("t", "a")).unwrap();
        let lease = granted(w.poll_with(busy(1_000_000)).unwrap());
        assert_eq!(lease.weight(), 1);
    }

    /// Automatic: a second holder needs load below the cores AND the ramp.
    #[test]
    fn auto_admits_a_second_holder_only_when_idle_and_after_the_ramp() {
        let d = scratch("auto");
        let t0 = 10_000_000;
        let mut a = Waiter::new_in(&d, Sizing::Auto, 1, Holder::here("t", "a")).unwrap();
        let _first = granted(a.poll_with(idle(t0)).unwrap());
        let mut b = Waiter::new_in(&d, Sizing::Auto, 1, Holder::here("t", "b")).unwrap();
        let ramp = waiting(b.poll_with(idle(t0 + 10_000)).unwrap()); // ramp not elapsed
        assert_eq!(ramp.reason, Some(WaitReason::Ramp));
        let load = waiting(b.poll_with(busy(t0 + 61_000)).unwrap()); // load too high
        assert_eq!(
            load.reason,
            Some(WaitReason::Load {
                load_tenths: 200,
                cores: 12,
                holder_jobs: 2
            })
        );
        assert_eq!(
            load.reason.unwrap().describe(),
            "load 20.0 >= 20 (12 cores + 4×2 holder jobs)"
        );
        let _second = granted(b.poll_with(idle(t0 + 61_000)).unwrap());
    }

    /// With every slot held, a waiter says so, whatever the load.
    #[test]
    fn a_full_host_says_its_slots_are_full() {
        let d = scratch("full");
        let mut a = Waiter::new_in(&d, Sizing::Fixed(1), 1, Holder::here("t", "a")).unwrap();
        let _held = granted(a.poll_with(idle(1_000_000)).unwrap());
        let mut b = Waiter::new_in(&d, Sizing::Fixed(1), 1, Holder::here("t", "b")).unwrap();
        let q = waiting(b.poll_with(idle(1_000_000)).unwrap());
        assert_eq!(q.reason, Some(WaitReason::SlotsFull { held: 1, cap: 1 }));
        assert_eq!(q.reason.unwrap().describe(), "slots full 1/1");
    }

    /// A sibling of a holder in the same process (one commit's checks)
    /// skips the ramp, but not the load test or the cap.
    #[test]
    fn a_sibling_of_a_holder_skips_the_ramp_but_not_the_load_test() {
        let d = scratch("sibling");
        let t0 = 70_000_000;
        let mut a = Waiter::new_in(&d, Sizing::Auto, 1, Holder::here("t", "a")).unwrap();
        let _a = granted(a.poll_with(idle(t0)).unwrap());
        let sibling = |now_ms, load1| Probe {
            load1: Some(load1),
            cores: 12,
            now_ms,
            own_held: 1,
        };
        let mut b = Waiter::new_in(&d, Sizing::Auto, 1, Holder::here("t", "b")).unwrap();
        waiting(b.poll_with(sibling(t0 + 1_000, 20.0)).unwrap()); // busy: still waits
        let _b = granted(b.poll_with(sibling(t0 + 1_000, 1.0)).unwrap()); // idle: no ramp
    }

    /// Three long jobs on an idle host: the ramp admits one per 60 s, so the
    /// third is still waiting at 90 s (the wrapper's default `--max-wait`,
    /// where it exits 75) and gets its slot at 120 s.
    #[test]
    fn the_ramp_keeps_a_third_waiter_queued_past_ninety_seconds() {
        let d = scratch("three");
        let t0 = 50_000_000;
        let mut a = Waiter::new_in(&d, Sizing::Auto, 1, Holder::here("t", "a")).unwrap();
        let _a = granted(a.poll_with(idle(t0)).unwrap());
        let mut b = Waiter::new_in(&d, Sizing::Auto, 1, Holder::here("t", "b")).unwrap();
        let mut c = Waiter::new_in(&d, Sizing::Auto, 1, Holder::here("t", "c")).unwrap();
        let _b = granted(b.poll_with(idle(t0 + 60_000)).unwrap());
        waiting(c.poll_with(idle(t0 + 90_000)).unwrap());
        let _c = granted(c.poll_with(idle(t0 + 120_000)).unwrap());
    }

    /// A process holding the admission lock (stopped, or stuck on I/O) never
    /// blocks a waiter: the poll answers at once, waiting.
    #[test]
    fn a_held_admission_lock_never_blocks_a_poll() {
        let d = scratch("admission");
        let mut w = Waiter::new_in(&d, Sizing::Fixed(1), 1, Holder::here("t", "a")).unwrap();
        let _stuck = platform::try_lock(&d.join("admission")).unwrap().unwrap();
        let started = Instant::now();
        let q = waiting(w.poll_with(idle(0)).unwrap());
        assert!(started.elapsed() < Duration::from_millis(500));
        assert_eq!(q, Queue::default());
    }

    /// The automatic cap follows the machine admission sees: half its
    /// cores, at least one.
    #[test]
    fn the_auto_cap_is_half_the_cores_at_least_one() {
        assert_eq!(Sizing::Auto.cap_for(12), 6);
        assert_eq!(Sizing::Auto.cap_for(2), 1);
        assert_eq!(Sizing::Auto.cap_for(1), 1);
        assert_eq!(Sizing::Fixed(3).cap_for(64), 3);
    }

    /// Fixed sizing ignores load and ramp, and stops at the cap.
    #[test]
    fn fixed_sizing_stops_at_the_cap() {
        let d = scratch("fixed");
        let mut a = Waiter::new_in(&d, Sizing::Fixed(1), 1, Holder::here("t", "a")).unwrap();
        let _l = granted(a.poll_with(busy(0)).unwrap());
        let mut b = Waiter::new_in(&d, Sizing::Fixed(1), 1, Holder::here("t", "b")).unwrap();
        let q = waiting(b.poll_with(idle(0)).unwrap());
        assert_eq!(q.holders.len(), 1, "{q:?}");
        assert_eq!(q.holders[0].command, "a");
    }

    /// Only the head of the queue may take a slot.
    #[test]
    fn only_the_oldest_ticket_is_served() {
        let d = scratch("order");
        let mut holder = Waiter::new_in(&d, Sizing::Fixed(1), 1, Holder::here("t", "h")).unwrap();
        let held = granted(holder.poll_with(idle(0)).unwrap());
        let mut first = Waiter::new_in(&d, Sizing::Fixed(1), 1, Holder::here("t", "1")).unwrap();
        let mut second = Waiter::new_in(&d, Sizing::Fixed(1), 1, Holder::here("t", "2")).unwrap();
        waiting(first.poll_with(idle(0)).unwrap());
        drop(held);
        let q = waiting(second.poll_with(idle(0)).unwrap());
        assert_eq!((q.position, q.waiting), (1, 2), "{q:?}");
        let _l = granted(first.poll_with(idle(0)).unwrap());
    }

    /// A waiter that went away leaves the queue; its ticket is not served.
    #[test]
    fn a_dropped_waiter_leaves_the_queue() {
        let d = scratch("dropped");
        let gone = Waiter::new_in(&d, Sizing::Fixed(1), 1, Holder::here("t", "gone")).unwrap();
        drop(gone);
        let mut next = Waiter::new_in(&d, Sizing::Fixed(1), 1, Holder::here("t", "next")).unwrap();
        let _l = granted(next.poll_with(idle(0)).unwrap());
    }

    /// Weight 2 is all or none.
    #[test]
    fn weight_is_all_or_none() {
        let d = scratch("weight");
        let mut one = Waiter::new_in(&d, Sizing::Fixed(2), 1, Holder::here("t", "1")).unwrap();
        let _l = granted(one.poll_with(idle(0)).unwrap());
        let mut two = Waiter::new_in(&d, Sizing::Fixed(2), 2, Holder::here("t", "2")).unwrap();
        waiting(two.poll_with(idle(0)).unwrap());
    }

    /// A record whose writer is gone is not a holder: the slot reads as an
    /// old client's.
    #[test]
    fn a_dead_holders_record_is_ignored() {
        let d = scratch("dead");
        safe_dir(&d).unwrap();
        let _held = platform::try_lock(&d.join("slot-0")).unwrap().unwrap();
        let dead = Holder {
            pid: i32::MAX as u32,
            tool: "t".into(),
            ..Holder::default()
        };
        fs::write(d.join("holders/slot-0.who"), dead.to_json()).unwrap();
        let s = status_in(&d, Sizing::Fixed(1)).unwrap();
        assert!(s.holders.is_empty(), "{s:?}");
        assert_eq!(s.unknown, 1);
    }

    /// A fresh directory is made 0700 and accepted; a 0755 one and a
    /// symlink are refused.
    #[test]
    fn only_a_private_directory_is_accepted() {
        use std::os::unix::fs::PermissionsExt;
        let d = scratch("private");
        assert_eq!(safe_dir(&d), Ok(()));
        fs::set_permissions(&d, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(safe_dir(&d).unwrap_err().contains("mode 755"));
        let link = scratch("link");
        std::os::unix::fs::symlink(&d, &link).unwrap();
        assert!(safe_dir(&link).unwrap_err().contains("symbolic link"));
        let _ = fs::remove_file(&link); // best-effort: scratch
        let _ = fs::remove_dir_all(&d); // best-effort: scratch
    }

    /// The record round-trips, escapes included.
    #[test]
    fn a_holder_record_round_trips() {
        let h = Holder {
            pid: 42,
            tool: "relais".into(),
            repo: "/w/a \"b\"".into(),
            command: "cargo test\tx".into(),
            session: "s".into(),
            started_ms: 7,
            jobs: Some(4),
        };
        assert_eq!(Holder::from_json(&h.to_json()), Some(h.clone()));
        // A record from an older greengate-slots has no `jobs`, and still reads.
        let old = Holder { jobs: None, ..h };
        assert!(!old.to_json().contains("jobs"));
        assert_eq!(Holder::from_json(&old.to_json()), Some(old));
    }

    /// Claude Code's session wins, then relais's, then the old name; an
    /// empty value counts as unset.
    #[test]
    fn the_session_comes_from_the_first_set_variable() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |k: &str| {
                pairs
                    .iter()
                    .find(|(n, _)| *n == k)
                    .map(|(_, v)| v.to_string())
            }
        };
        assert_eq!(
            session_from(env(&[
                ("CLAUDE_SESSION_ID", "old"),
                ("RELAIS_SESSION_ID", "r"),
                ("CLAUDE_CODE_SESSION_ID", "cc")
            ])),
            "cc"
        );
        assert_eq!(
            session_from(env(&[
                ("CLAUDE_CODE_SESSION_ID", ""),
                ("RELAIS_SESSION_ID", "r")
            ])),
            "r"
        );
        assert_eq!(session_from(env(&[("CLAUDE_SESSION_ID", "old")])), "old");
        assert_eq!(session_from(env(&[])), "");
    }

    /// The listing shows a session's first 8 characters, `-` for none.
    #[test]
    fn a_short_session_is_eight_characters() {
        let h = |s: &str| Holder {
            session: s.into(),
            ..Holder::default()
        };
        assert_eq!(h("94649c5c-bf8a-4ec0").short_session(), "94649c5c");
        assert_eq!(h("abc").short_session(), "abc");
        assert_eq!(h("").short_session(), "-");
    }

    /// The width a weight buys: relais's formula, never below one job.
    #[test]
    fn parallelism_is_weight_times_cores_over_cap() {
        assert_eq!(parallelism(1, 12, 6), 2);
        assert_eq!(parallelism(2, 12, 6), 4);
        assert_eq!(parallelism(3, 12, 6), 6);
        assert_eq!(parallelism(1, 1, 6), 1);
        assert_eq!(parallelism(1, 12, 0), 12);
        assert_eq!(parallelism(u32::MAX, u32::MAX, 1), u32::MAX);
    }

    fn env_of<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| {
            pairs
                .iter()
                .find(|(key, _)| *key == k)
                .map(|(_, v)| v.to_string())
        }
    }

    /// Nothing set: every key at the width.
    #[test]
    fn an_unset_environment_gets_every_key() {
        let set = width_env(2, env_of(&[]));
        assert_eq!(
            set,
            vec![
                ("MAKEFLAGS", "-j2".to_string()),
                ("CARGO_BUILD_JOBS", "2".to_string()),
                ("RUST_TEST_THREADS", "2".to_string()),
                ("NEXTEST_TEST_THREADS", "2".to_string()),
                ("VITEST_MAX_WORKERS", "2".to_string()),
                ("GOMAXPROCS", "2".to_string()),
            ]
        );
    }

    /// A key the check or a parent set wins; an empty one is replaced.
    #[test]
    fn a_preset_key_wins_and_an_empty_one_does_not() {
        let set = width_env(
            2,
            env_of(&[
                ("CARGO_BUILD_JOBS", "9"),
                ("RUST_TEST_THREADS", ""),
                ("MAKEFLAGS", "-j9"),
            ]),
        );
        assert!(
            !set.iter().any(|(k, _)| *k == "CARGO_BUILD_JOBS"),
            "{set:?}"
        );
        assert!(!set.iter().any(|(k, _)| *k == "MAKEFLAGS"), "{set:?}");
        assert!(
            set.contains(&("RUST_TEST_THREADS", "2".to_string())),
            "{set:?}"
        );
    }

    /// A MAKEFLAGS with flags but no job bound gains one; a jobserver, a
    /// `--jobs` or make's own short form (`j4`) already bounds it.
    #[test]
    fn makeflags_without_a_job_bound_gains_one() {
        let set = width_env(3, env_of(&[("MAKEFLAGS", "--no-print-directory")]));
        assert!(
            set.contains(&("MAKEFLAGS", "--no-print-directory -j3".to_string())),
            "{set:?}"
        );
        for bounded in [
            "--jobserver-auth=3,4",
            "--jobs=2",
            "j4 --no-print-directory",
            "-s -j8",
        ] {
            let set = width_env(3, env_of(&[("MAKEFLAGS", bounded)]));
            assert!(
                !set.iter().any(|(k, _)| *k == "MAKEFLAGS"),
                "{bounded}: {set:?}"
            );
        }
        // `s` alone is make's short form for -s, not a job bound.
        let set = width_env(3, env_of(&[("MAKEFLAGS", "s")]));
        assert!(set.contains(&("MAKEFLAGS", "s -j3".to_string())), "{set:?}");
    }

    /// The command's own setting is read before the inherited one, and an
    /// `env_remove` counts as unset.
    #[test]
    fn bound_width_reads_the_commands_own_env_first() {
        let mut cmd = std::process::Command::new("true");
        cmd.env("CARGO_BUILD_JOBS", "7")
            .env_remove("RUST_TEST_THREADS");
        bound_width(&mut cmd, 2);
        let envs: Vec<(String, Option<String>)> = cmd
            .get_envs()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    v.map(|v| v.to_string_lossy().into_owned()),
                )
            })
            .collect();
        let get = |key: &str| {
            envs.iter()
                .find(|(k, _)| k == key)
                .and_then(|(_, v)| v.clone())
        };
        assert_eq!(get("CARGO_BUILD_JOBS").as_deref(), Some("7"));
        assert_eq!(get("RUST_TEST_THREADS").as_deref(), Some("2"));
    }

    /// On a four-core machine a weight-2 waiter under automatic sizing gets
    /// cap 2 and so 2 x 4 / 2 = 4 jobs, from the machine it was given, not
    /// from the host it runs on.
    #[test]
    fn a_waiter_sizes_from_the_machine_it_is_given() {
        let d = scratch("machine");
        let w = Waiter::new_in_on(
            &d,
            Sizing::Auto,
            2,
            Holder::here("t", "a"),
            Machine { cores: 4 },
        )
        .unwrap();
        assert_eq!(w.holder.jobs, Some(4));
        drop(w);
        let _ = fs::remove_dir_all(&d); // best-effort: scratch
    }

    /// A waiter sized for four cores probes with four, so poll() refuses at
    /// load 4.0 whatever the host has (the override reaches the load test).
    #[test]
    fn a_waiter_probes_with_the_machine_it_was_sized_for() {
        let d = scratch("probe-machine");
        let mut first = Waiter::new_in_on(
            &d,
            Sizing::Auto,
            1,
            Holder::here("t", "a"),
            Machine { cores: 4 },
        )
        .unwrap();
        let _held = granted(first.poll_with(idle(1_000_000)).unwrap());
        let w = Waiter::new_in_on(
            &d,
            Sizing::Auto,
            1,
            Holder::here("t", "b"),
            Machine { cores: 4 },
        )
        .unwrap();
        assert_eq!(w.probe().cores, 4);
        let mut w = w;
        // One holder at jobs 2 on 4 cores: the limit is 4 + 4×2 = 12.
        let at_four = Probe {
            load1: Some(12.0),
            now_ms: 1_000_000 + 61_000,
            ..w.probe()
        };
        let q = waiting(w.poll_with(at_four).unwrap());
        assert_eq!(
            q.reason,
            Some(WaitReason::Load {
                load_tenths: 120,
                cores: 4,
                holder_jobs: 2
            })
        );
        let _ = fs::remove_dir_all(&d); // best-effort: scratch
    }

    /// Status reports the cap for the machine it is given, not the host's.
    #[test]
    fn status_caps_from_the_machine_it_is_given() {
        let d = scratch("status-machine");
        let st = status_in_on(&d, Sizing::Auto, Machine { cores: 4 }).unwrap();
        assert_eq!(st.cap, 2);
        let st = status_in_on(&d, Sizing::Auto, Machine { cores: 12 }).unwrap();
        assert_eq!(st.cap, 6);
        let _ = fs::remove_dir_all(&d); // best-effort: scratch
    }

    /// A waiter's record carries the width its weight buys.
    #[test]
    fn a_waiters_record_carries_its_width() {
        let d = scratch("width");
        let w = Waiter::new_in(&d, Sizing::Fixed(4), 2, Holder::here("t", "a")).unwrap();
        assert_eq!(w.holder.jobs, Some(parallelism(2, cores(), 4)));
        drop(w);
        let _ = fs::remove_dir_all(&d); // best-effort: scratch
    }

    /// A lease removes its records, and its slots are free again.
    #[test]
    fn a_dropped_lease_frees_its_slots_and_records() {
        let d = scratch("lease");
        let mut w = Waiter::new_in(&d, Sizing::Fixed(1), 1, Holder::here("t", "a")).unwrap();
        let lease = granted(w.poll_with(idle(0)).unwrap());
        assert!(d.join("holders/slot-0.who").exists());
        drop(lease);
        assert!(!d.join("holders/slot-0.who").exists());
        let mut again = Waiter::new_in(&d, Sizing::Fixed(1), 1, Holder::here("t", "b")).unwrap();
        let _l = granted(again.poll_with(idle(0)).unwrap());
    }

    #[test]
    fn rename_rewrites_every_who_file_and_resets_the_start() {
        let dir = scratch("rename");
        let mut waiter = Waiter::new_in(&dir, Sizing::Fixed(2), 2, Holder::here("t", "a")).unwrap();
        let lease = granted(waiter.poll_with(idle(0)).unwrap());
        let record = |index: usize| {
            let text = fs::read_to_string(dir.join(format!("holders/slot-{index}.who"))).unwrap();
            Holder::from_json(&text).unwrap()
        };
        let granted_jobs: Vec<Option<u32>> = (0..2).map(|index| record(index).jobs).collect();
        let mut next = Holder::here("t", "b");
        next.repo = "/some/repo".into();
        next.session = "sess".into();
        next.started_ms = 1;
        lease.rename(&next).unwrap();
        for (index, jobs_at_grant) in granted_jobs.iter().enumerate() {
            let got = record(index);
            assert_eq!(got.command, "b");
            assert_eq!(got.repo, "/some/repo");
            assert_eq!(got.session, "sess");
            assert!(got.started_ms > 1);
            // The width written at the grant survives the rename.
            assert_eq!(got.jobs, *jobs_at_grant);
        }
        let hidden: Vec<_> = fs::read_dir(dir.join("holders"))
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with('.'))
            .collect();
        assert!(hidden.is_empty());
    }

    #[test]
    fn a_dead_pids_hidden_holder_temp_is_removed_at_the_next_grant() {
        let dir = scratch("dead-temp");
        let mut first = Waiter::new_in(&dir, Sizing::Fixed(2), 1, Holder::here("t", "a")).unwrap();
        let _held = granted(first.poll_with(idle(0)).unwrap());
        let dead = dir.join(format!("holders/.slot-0.who.{}.tmp", i32::MAX));
        // On slot 0, which `first` holds: the grant below writes slot 1's
        // record through this process's own temp name for slot 1.
        let live = dir.join(format!("holders/.slot-0.who.{}.tmp", std::process::id()));
        fs::write(&dead, "{}").unwrap();
        fs::write(&live, "{}").unwrap();
        let mut second = Waiter::new_in(&dir, Sizing::Fixed(2), 1, Holder::here("t", "b")).unwrap();
        let _also = granted(second.poll_with(idle(0)).unwrap());
        assert!(!dead.exists());
        assert!(live.exists());
    }

    /// A holder that exited without dropping its lease leaves its record; the
    /// next grant removes it, so a reused pid never makes it count again.
    #[test]
    fn a_gone_holders_record_is_removed_at_the_next_grant() {
        let dir = scratch("dead-record");
        let mut first = Waiter::new_in(&dir, Sizing::Fixed(3), 1, Holder::here("t", "a")).unwrap();
        let held = granted(first.poll_with(idle(0)).unwrap());
        // Slot 2 is free: its holder died with its lease held, and its pid
        // is in use again (this process's), so only the free flock tells.
        // The grant below takes slot 1, so only the sweep removes this.
        let reused = with_jobs("gone", 2);
        let stale = dir.join("holders/slot-2.who");
        fs::write(&stale, reused.to_json_with(Some("gone-0"))).unwrap();
        let mut second = Waiter::new_in(&dir, Sizing::Fixed(3), 1, Holder::here("t", "b")).unwrap();
        let also = granted(second.poll_with(idle(0)).unwrap());
        assert_eq!(also.who[0], dir.join("holders/slot-1.who"));
        assert!(!stale.exists(), "the gone holder's record is removed");
        assert!(held.who[0].exists(), "a live holder's record stays");
    }

    #[test]
    fn rename_that_cannot_write_a_record_names_the_path() {
        let dir = scratch("rename-unwritable");
        let mut waiter = Waiter::new_in(&dir, Sizing::Fixed(1), 1, Holder::here("t", "a")).unwrap();
        let lease = granted(waiter.poll_with(idle(0)).unwrap());
        let holders = dir.join("holders");
        // A directory where the temp record goes makes the write fail for
        // any user, root included, which a permission mode would not.
        let temp = holders.join(format!(".slot-0.who.{}.tmp", std::process::id()));
        fs::create_dir(&temp).unwrap();
        let outcome = lease.rename(&Holder::here("t", "b"));
        fs::remove_dir(&temp).unwrap();
        let error = outcome.expect_err("a record whose temp path is a directory cannot be written");
        assert!(
            error.to_string().contains(&holders.display().to_string()),
            "{error}"
        );
    }

    #[test]
    fn a_reader_never_sees_an_unknown_record_while_rename_runs() {
        let dir = scratch("rename-race");
        let mut waiter = Waiter::new_in(&dir, Sizing::Fixed(1), 1, Holder::here("t", "a")).unwrap();
        let lease = granted(waiter.poll_with(idle(0)).unwrap());
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let reader_stop = stop.clone();
        let dir = dir.clone();
        let reader = std::thread::spawn(move || {
            let mut unknown = 0;
            while !reader_stop.load(Ordering::SeqCst) {
                let (_, bad) = read_holders(&dir, &[0]);
                unknown += bad;
            }
            unknown
        });
        for round in 0..2000 {
            lease
                .rename(&Holder::here("t", &format!("gate-{round}")))
                .unwrap();
        }
        stop.store(true, Ordering::SeqCst);
        assert_eq!(reader.join().unwrap(), 0);
    }

    #[test]
    fn record_wait_named_writes_name_pid_session_and_context() {
        let dir = scratch("named");
        fs::create_dir_all(&dir).unwrap();
        record_wait_named(
            &dir,
            "amont",
            WaitContext {
                name: Some("cargo-test"),
                place: WaitPlace::Prestage,
                cap: Some(3),
                cores: Some(5),
                end: None,
            },
            1,
            Some(3),
            0,
            "handed",
        );
        let line = fs::read_to_string(dir.join("waits.jsonl")).unwrap();
        assert_eq!(raw(&line, "name").as_deref(), Some("cargo-test"));
        // The sized-for count, not the detected one, and the cap with it.
        assert_eq!(raw(&line, "cores").as_deref(), Some("5"));
        assert_eq!(raw(&line, "cap").as_deref(), Some("3"));
        assert_eq!(raw(&line, "pid"), Some(std::process::id().to_string()));
        assert!(line.contains("\"rehearsal\":true"));
        assert!(line.contains("\"prestage\":true"));
        assert_eq!(raw(&line, "outcome").as_deref(), Some("handed"));
        if !session_id().is_empty() {
            assert_eq!(raw(&line, "session"), Some(session_id()));
        }
    }

    #[test]
    fn record_wait_keeps_its_old_fields_and_gains_the_pid() {
        let dir = scratch("plain");
        fs::create_dir_all(&dir).unwrap();
        record_wait(&dir, "relais", 1, 12, "admitted");
        let line = fs::read_to_string(dir.join("waits.jsonl")).unwrap();
        assert!(line.contains("\"tool\":\"relais\""));
        assert!(line.contains("\"waited_ms\":12"));
        assert!(line.contains("\"outcome\":\"admitted\""));
        assert_eq!(raw(&line, "pid"), Some(std::process::id().to_string()));
        assert!(!line.contains("\"name\""));
        assert!(!line.contains("\"rehearsal\""));
        assert!(!line.contains("\"prestage\""));
    }

    /// A holder of `jobs` jobs, running `command`.
    fn with_jobs(command: &str, jobs: u32) -> Holder {
        Holder {
            jobs: Some(jobs),
            ..Holder::here("t", command)
        }
    }

    /// A granted lease of `weight` slots in `dir`.
    fn take(dir: &Path, weight: u32, holder: Holder) -> Lease {
        let mut w = Waiter::new_in(dir, Sizing::Fixed(4), weight, holder).unwrap();
        granted(w.poll_with(idle(0)).unwrap())
    }

    /// The lines of `waits.jsonl`.
    fn journal(dir: &Path) -> Vec<String> {
        fs::read_to_string(dir.join("waits.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// The last wait line (one with `waited_ms`).
    fn last_wait(dir: &Path) -> String {
        journal(dir)
            .into_iter()
            .rev()
            .find(|l| l.contains("\"waited_ms\""))
            .expect("a wait line")
    }

    #[test]
    fn an_admitted_line_counts_its_own_lease_and_a_timeout_only_the_others() {
        let dir = scratch("occupancy");
        let _other = take(&dir, 2, with_jobs("other", 2));
        let mine = take(&dir, 1, with_jobs("mine", 3));
        record_wait(&dir, "t", 1, 0, "admitted");
        let line = last_wait(&dir);
        assert_eq!(raw(&line, "holders").as_deref(), Some("3"), "{line}");
        // The weight-2 lease's jobs once, plus this lease's.
        assert_eq!(raw(&line, "holder_jobs").as_deref(), Some("5"), "{line}");
        drop(mine);
        record_wait(&dir, "t", 1, 900, "timeout");
        let line = last_wait(&dir);
        assert_eq!(raw(&line, "holders").as_deref(), Some("2"), "{line}");
        assert_eq!(raw(&line, "holder_jobs").as_deref(), Some("2"), "{line}");
    }

    #[test]
    fn two_leases_from_one_cloned_holder_count_apart() {
        let dir = scratch("siblings");
        let holder = with_jobs("sibling", 2);
        let _a = take(&dir, 1, holder.clone());
        let _b = take(&dir, 1, holder);
        assert_eq!(occupancy(&dir, 2), Some((2, 4)));
    }

    #[test]
    fn a_lease_seen_mid_rename_counts_its_jobs_once() {
        let dir = scratch("mid-rename");
        let lease = take(&dir, 2, with_jobs("before", 2));
        // One record renamed, the other not yet: what a reader sees between
        // the two writes of `Lease::rename`.
        let after = with_jobs("after", 2);
        write_record(&lease.who[0], &record_json(&after, &lease.id)).unwrap();
        assert_eq!(occupancy(&dir, 2), Some((2, 2)));
    }

    #[test]
    fn an_empty_directory_holds_nothing_and_an_unreadable_record_is_unknown() {
        let dir = scratch("empty");
        fs::create_dir_all(&dir).unwrap();
        assert_eq!(occupancy(&dir, 2), Some((0, 0)), "no holders/ at all");
        fs::create_dir_all(dir.join("holders")).unwrap();
        assert_eq!(occupancy(&dir, 2), Some((0, 0)));
        record_wait(&dir, "t", 1, 0, "admitted");
        assert!(last_wait(&dir).contains("\"holders\":0,\"holder_jobs\":0"));
        fs::write(dir.join("holders/slot-0.who"), "{").unwrap();
        assert_eq!(occupancy(&dir, 2), None);
        record_wait(&dir, "t", 1, 0, "timeout");
        assert!(last_wait(&dir).contains("\"holders\":null,\"holder_jobs\":null"));
    }

    #[test]
    fn every_wait_outcome_carries_waited_ms() {
        let dir = scratch("outcomes");
        fs::create_dir_all(&dir).unwrap();
        let outcomes = ["admitted", "handed", "timeout", "unqueued", "deferred"];
        for outcome in outcomes {
            record_wait(&dir, "t", 1, 7, outcome);
        }
        let lines = journal(&dir);
        assert_eq!(lines.len(), outcomes.len());
        for line in lines {
            assert!(line.contains("\"waited_ms\":7"), "{line}");
        }
    }

    #[test]
    fn the_lease_id_is_in_the_record_but_not_in_the_holder() {
        let dir = scratch("lease-id");
        let lease = take(&dir, 1, with_jobs("a", 2));
        let text = fs::read_to_string(&lease.who[0]).unwrap();
        assert_eq!(raw(&text, "lease"), Some(lease.id.clone()));
        let holder = Holder::from_json(&text).unwrap();
        assert!(
            !holder.to_json().contains("\"lease\""),
            "slots --json unchanged"
        );
        lease.rename(&Holder::here("t", "renamed")).unwrap();
        let text = fs::read_to_string(&lease.who[0]).unwrap();
        assert_eq!(
            raw(&text, "lease"),
            Some(lease.id.clone()),
            "kept by rename"
        );
    }

    #[test]
    fn a_dropped_lease_appends_one_release_line() {
        let dir = scratch("release");
        let before = now_ms();
        // The probe's clock is faked; the release line must not use it.
        let mut w = Waiter::new_in(&dir, Sizing::Fixed(1), 1, with_jobs("a", 2)).unwrap();
        let lease = granted(w.poll_with(idle(0)).unwrap());
        std::thread::sleep(Duration::from_millis(200));
        lease.rename(&Holder::here("t", "renamed")).unwrap();
        drop(lease);
        let lines = journal(&dir);
        let released: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("\"outcome\":\"released\""))
            .collect();
        assert_eq!(released.len(), 1, "{lines:?}");
        let line = released[0];
        let held: u128 = raw(line, "held_ms").unwrap().parse().unwrap();
        assert!((200..2000).contains(&held), "{line}");
        let granted_ms: u128 = raw(line, "granted_ms").unwrap().parse().unwrap();
        assert!(granted_ms >= before && granted_ms <= now_ms(), "{line}");
        assert_eq!(raw(line, "command").as_deref(), Some("a"), "the grant's");
        assert_eq!(raw(line, "jobs").as_deref(), Some("2"));
        assert_eq!(raw(line, "weight").as_deref(), Some("1"));
        assert_eq!(raw(line, "pid"), Some(std::process::id().to_string()));
        assert!(!line.contains("waited_ms"));
    }

    #[test]
    fn a_lease_dropped_after_its_directory_went_away_does_not_panic() {
        let dir = scratch("gone");
        let lease = take(&dir, 1, with_jobs("a", 2));
        fs::remove_dir_all(&dir).unwrap();
        drop(lease);
        assert!(!dir.exists());
    }

    // --- ADR-0022: admission counts the holders, waits end on a stall ---

    /// On 6 cores, one holder at `jobs 2`, and a second waiter in line.
    fn one_holder_and_a_waiter(tag: &str) -> (PathBuf, Lease, Waiter) {
        let dir = scratch(tag);
        let first = Probe {
            load1: Some(1.0),
            cores: 6,
            now_ms: 30_000_000,
            own_held: 0,
        };
        let mut a = Waiter::new_in(&dir, Sizing::Auto, 1, with_jobs("a", 2)).unwrap();
        let lease = granted(a.poll_with(first).unwrap());
        let b = Waiter::new_in(&dir, Sizing::Auto, 1, with_jobs("b", 2)).unwrap();
        (dir, lease, b)
    }

    /// t01: one holder at jobs 2 measured 12.3 by itself; the limit is
    /// 6 + 4×2 = 14, so the next job is admitted (it was refused at >= 6).
    #[test]
    fn a_holders_own_load_does_not_shut_the_next_slot() {
        let dir = scratch("t01-admit");
        let t0 = 30_000_000;
        let six = |load1, now_ms| Probe {
            load1: Some(load1),
            cores: 6,
            now_ms,
            own_held: 0,
        };
        let mut a = Waiter::new_in(&dir, Sizing::Auto, 1, with_jobs("a", 2)).unwrap();
        let _a = granted(a.poll_with(six(1.0, t0)).unwrap());
        let mut b = Waiter::new_in(&dir, Sizing::Auto, 1, with_jobs("b", 2)).unwrap();
        let _b = granted(b.poll_with(six(12.3, t0 + 61_000)).unwrap());
    }

    /// t01's pair: outside load of 300 still holds the job back, and says
    /// the limit it was held to.
    #[test]
    fn outside_load_still_holds_the_next_job_back() {
        let (_dir, _lease, mut b) = one_holder_and_a_waiter("t01-refuse");
        let q = waiting(
            b.poll_with(Probe {
                load1: Some(300.0),
                cores: 6,
                now_ms: 30_000_000 + 62_000,
                own_held: 0,
            })
            .unwrap(),
        );
        assert_eq!(
            q.reason,
            Some(WaitReason::Load {
                load_tenths: 3000,
                cores: 6,
                holder_jobs: 2
            })
        );
        assert_eq!(
            q.reason.unwrap().describe(),
            "load 300.0 >= 14 (6 cores + 4×2 holder jobs)"
        );
    }

    /// A held slot whose record names no jobs (an older writer) counts one
    /// slot's width, in the load test and in the journal alike.
    #[test]
    fn a_record_without_jobs_counts_one_slots_width_everywhere() {
        let dir = scratch("missing-jobs");
        let lease = take(
            &dir,
            1,
            Holder {
                jobs: None,
                ..Holder::here("t", "old")
            },
        );
        // `take` sized it; strip `jobs` from the record as an older writer would.
        let text = fs::read_to_string(&lease.who[0]).unwrap();
        let stripped = text.replace(&format!(",\"jobs\":{}", lease_jobs_of(&text)), "");
        assert!(!stripped.contains("\"jobs\""), "{stripped}");
        fs::write(&lease.who[0], stripped).unwrap();
        assert_eq!(held_jobs(&dir, &[0], 2), 2);
        assert_eq!(occupancy(&dir, 2), Some((1, 2)));
        // A held slot with no record at all counts the same in the load test.
        fs::remove_file(&lease.who[0]).unwrap();
        assert_eq!(held_jobs(&dir, &[0], 2), 2);
    }

    fn lease_jobs_of(text: &str) -> String {
        raw(text, "jobs").expect("a sized record names its jobs")
    }

    fn at(position: usize) -> Queue {
        Queue {
            position,
            waiting: position + 1,
            reason: Some(WaitReason::Behind { ahead: position }),
            ..Queue::default()
        }
    }

    /// t02's red test: at position 10, one admission every 60 s and a 90 s
    /// budget, the line is moving, so after 400 s the wait has not stalled.
    #[test]
    fn a_wait_in_a_moving_line_never_stalls() {
        let wall0: u128 = 1_800_000_000_000;
        let mut stall = Stall::new(STALL);
        let mut admitted = wall0;
        for step in 0..=40u64 {
            let elapsed = Duration::from_secs(step * 10);
            if step > 0 && step % 6 == 0 {
                admitted = wall0 + u128::from(step) * 10_000; // one admission a minute
            }
            let position = 10 - (step / 6) as usize;
            let now = wall0 + u128::from(step) * 10_000;
            assert!(
                !stall.observe_at(&at(position), elapsed, now, admitted),
                "stalled at {}s while the line moved",
                step * 10
            );
        }
    }

    /// t02's pair: no admission and no change of place for the window stalls.
    #[test]
    fn a_line_with_no_progress_for_the_window_stalls() {
        let wall0: u128 = 1_800_000_000_000;
        let mut stall = Stall::new(STALL);
        assert!(!stall.observe_at(&at(3), Duration::ZERO, wall0, wall0 - 5_000));
        assert!(!stall.observe_at(
            &at(3),
            Duration::from_secs(299),
            wall0 + 299_000,
            wall0 - 5_000
        ));
        assert!(stall.observe_at(
            &at(3),
            Duration::from_secs(300),
            wall0 + 300_000,
            wall0 - 5_000
        ));
    }

    /// Moving up one place is progress, even with no admission seen (the one
    /// ahead left the queue).
    #[test]
    fn a_change_of_place_restarts_the_clock() {
        let wall0: u128 = 1_800_000_000_000;
        let mut stall = Stall::new(STALL);
        stall.observe_at(&at(3), Duration::ZERO, wall0, 0);
        assert!(!stall.observe_at(&at(2), Duration::from_secs(250), wall0 + 250_000, 0));
        assert!(!stall.observe_at(&at(2), Duration::from_secs(500), wall0 + 500_000, 0));
        assert!(stall.observe_at(&at(2), Duration::from_secs(550), wall0 + 550_000, 0));
    }

    /// A poll that missed the admission lock saw nothing: its zero position
    /// is not a change of place.
    #[test]
    fn a_lock_miss_poll_moves_no_mark() {
        let mut stall = Stall::new(Duration::from_secs(10));
        stall.observe_at(&at(4), Duration::ZERO, 1_000_000, 0);
        stall.observe_at(&Queue::default(), Duration::from_secs(5), 1_005_000, 0);
        assert!(stall.observe_at(&at(4), Duration::from_secs(10), 1_010_000, 0));
    }

    /// A stamp ahead of now (another process's clock, a step back) counts as
    /// an admission just now: no panic, no instant stall.
    #[test]
    fn a_future_admission_stamp_is_no_stall_and_no_panic() {
        let stall = Stall::new(STALL);
        let idle = stall.idle_at(Duration::from_secs(400), 1_000_000, 9_000_000);
        assert_eq!(idle, Duration::ZERO);
    }

    /// The wall clock jumping forward (a laptop's sleep, a clock step) cannot
    /// stall a waiter whose own place changed recently: that clock is
    /// monotonic.
    #[test]
    fn a_forward_wall_clock_jump_does_not_stall_a_moving_waiter() {
        let mut stall = Stall::new(STALL);
        stall.observe_at(&at(5), Duration::ZERO, 1_000_000, 900_000);
        stall.observe_at(&at(4), Duration::from_secs(60), 1_060_000, 900_000);
        // An hour of wall time, 30 s of monotonic time.
        assert!(!stall.observe_at(&at(4), Duration::from_secs(90), 4_690_000, 900_000));
    }

    #[test]
    fn the_stall_window_is_three_hundred_seconds_unless_the_seam_sets_it() {
        assert_eq!(Stall::window_from(None), (STALL, None));
        assert_eq!(
            Stall::window_from(Some("5")),
            (Duration::from_secs(5), None)
        );
        for bad in ["", "0", "-1", "5m"] {
            let (window, warning) = Stall::window_from(Some(bad));
            assert_eq!(window, STALL, "{bad:?}");
            assert!(warning.is_some_and(|w| w.contains(STALL_ENV)), "{bad:?}");
        }
    }

    #[test]
    fn a_wait_that_ended_says_why() {
        let dir = scratch("end");
        fs::create_dir_all(&dir).unwrap();
        record_wait_named(
            &dir,
            "t",
            WaitContext {
                end: Some(WaitEnd::Stalled),
                ..WaitContext::default()
            },
            1,
            None,
            300_000,
            "timeout",
        );
        assert!(last_wait(&dir).contains("\"outcome\":\"timeout\",\"end\":\"stalled\""));
        record_wait(&dir, "t", 1, 0, "admitted");
        assert!(!last_wait(&dir).contains("\"end\""));
    }

    /// A scripted line for [`wait_in_line_with`]: each poll's answer from
    /// the script, the clock moving 10 s per poll.
    struct Line {
        polls: std::collections::VecDeque<io::Result<Poll>>,
        elapsed: Duration,
        admitted_ms: u128,
    }

    const WALL0: u128 = 1_800_000_000_000;

    fn waiting_at(position: usize) -> io::Result<Poll> {
        Ok(Poll::Waiting(at(position)))
    }

    fn run_line(
        bounds: WaitBounds,
        polls: Vec<io::Result<Poll>>,
        admit_every_secs: Option<u64>,
    ) -> Result<Lease, LineEnd> {
        let line = std::cell::RefCell::new(Line {
            polls: polls.into(),
            elapsed: Duration::ZERO,
            admitted_ms: WALL0,
        });
        wait_in_line_with(
            bounds,
            || {
                let mut line = line.borrow_mut();
                line.elapsed += Duration::from_secs(10);
                if let Some(every) = admit_every_secs {
                    if line.elapsed.as_secs() % every == 0 {
                        line.admitted_ms = WALL0 + line.elapsed.as_millis();
                    }
                }
                line.polls
                    .pop_front()
                    .unwrap_or_else(|| Err(io::Error::other("the script ran out")))
            },
            || {
                let line = line.borrow();
                (
                    line.elapsed,
                    WALL0 + line.elapsed.as_millis(),
                    line.admitted_ms,
                )
            },
            |_| {},
            |_| {},
        )
    }

    /// The shared loop, t02's red test: at position 10, an admission a
    /// minute; a budget would have cut it short, the stall rule never does,
    /// and only the ceiling ends it.
    #[test]
    fn the_shared_loop_waits_while_the_line_moves() {
        let polls = (0..80)
            .map(|step| waiting_at(10 - (step / 6).min(9)))
            .collect();
        let bounds = WaitBounds {
            window: Some(STALL),
            ceiling: Duration::from_secs(600),
        };
        match run_line(bounds, polls, Some(60)) {
            Err(LineEnd::Ceiling { waited }) => assert!(waited >= Duration::from_secs(600)),
            other => panic!("expected the ceiling, got {other:?}"),
        }
    }

    #[test]
    fn the_shared_loop_ends_a_stalled_line_and_says_where() {
        let polls = (0..60).map(|_| waiting_at(3)).collect();
        let bounds = WaitBounds {
            window: Some(Duration::from_secs(60)),
            ceiling: Duration::from_secs(600),
        };
        match run_line(bounds, polls, None) {
            Err(LineEnd::Stalled { window, position }) => {
                assert_eq!((window, position), (Duration::from_secs(60), 3));
            }
            other => panic!("expected a stall, got {other:?}"),
        }
    }

    /// A window as long as the ceiling never ends the wait first.
    #[test]
    fn a_window_as_long_as_the_ceiling_leaves_the_ceiling_to_end_it() {
        let polls = (0..20).map(|_| waiting_at(1)).collect();
        let bounds = WaitBounds {
            window: Some(Duration::from_secs(90)),
            ceiling: Duration::from_secs(90),
        };
        assert!(matches!(
            run_line(bounds, polls, None),
            Err(LineEnd::Ceiling { .. })
        ));
    }

    /// Two failed polls are a hiccup; the third in a row ends the wait.
    #[test]
    fn three_failed_polls_in_a_row_end_the_wait() {
        let fail = || Err(io::Error::other("busy disk"));
        let bounds = WaitBounds {
            window: None,
            ceiling: Duration::from_secs(600),
        };
        // Two failures, then a slot: granted, not ended.
        let dir = scratch("poll-failures");
        let mut waiter = Waiter::new_in(&dir, Sizing::Fixed(1), 1, Holder::here("t", "x")).unwrap();
        let grant = waiter.poll_with(idle(1_000_000)).unwrap();
        assert!(run_line(bounds, vec![fail(), fail(), Ok(grant)], None).is_ok());
        // A good poll resets the count: two failures, a wait, two more
        // failures, then waiting until the ceiling, which is what ends it.
        let mut polls = vec![fail(), fail(), waiting_at(1), fail(), fail()];
        polls.extend((0..70).map(|_| waiting_at(1)));
        assert!(matches!(
            run_line(bounds, polls, None),
            Err(LineEnd::Ceiling { .. })
        ));
        // Three in a row end it.
        let polls = vec![fail(), fail(), waiting_at(1), fail(), fail(), fail()];
        match run_line(bounds, polls, None) {
            Err(LineEnd::PollFailed { error }) => assert_eq!(error.to_string(), "busy disk"),
            other => panic!("expected the poll failure, got {other:?}"),
        }
    }
}
