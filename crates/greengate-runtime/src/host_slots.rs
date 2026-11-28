//! Host slots for heavy checks (ADR-0014, `hooks.host-concurrency`).
//!
//! Several worktrees, sessions and agents on one machine each run their own
//! clippy, test suite and type checker, and each of those already uses every
//! core. A check that compiles or executes the product is therefore Heavy,
//! and takes a host slot before its first tool runs. The protocol (the slot
//! directory, the lock, the queue, admission, the holder record) is the
//! `greengate-slots` crate, the one implementation relais links too; this module
//! is the hook's half: which checks are Heavy, the busy policy, deferral, and
//! the queue row.
//!
//! The slot is taken LAZILY, at the first tool spawn of a heavy check, not
//! when the check starts: a clippy with no Rust staged returns at once and
//! must not wait behind somebody else's suite to do nothing.

use crate::names::BIN;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub use greenharness_slots::{DIR_ENV, HELD_ENV};

/// The checks that compile or execute the product. Pinned by a registry
/// test: every name here is a built-in.
pub const HEAVY: &[&str] = &[
    "pre-commit-clippy",
    "pre-commit-go-vet",
    "pre-commit-pyright",
    "pre-push-run-tests-js",
    "pre-push-cargo-test",
    "pre-push-go-test",
    "pre-push-pytest",
];

/// Whether a check is Heavy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weight {
    Light,
    Heavy,
}

pub fn weight_of(name: &str) -> Weight {
    if HEAVY.contains(&name) {
        Weight::Heavy
    } else {
        Weight::Light
    }
}

/// Why a heavy check ran without a slot.
#[derive(Debug)]
pub enum Reason {
    /// The slot directory is not one this user alone owns.
    DirUnsafe(String),
    /// No `flock` here: Windows, the stated divergence.
    Unsupported,
    /// The queue could not be joined: a slot file could not be opened or
    /// locked for a reason other than "another holder has it". The slots are
    /// unusable, as if off.
    Io(String),
    /// The wait in line ended (ADR-0022): a stall, the ceiling, or failed
    /// polls. Nothing runs unqueued because of it: the check defers.
    Ended(greenharness_slots::LineEnd),
}

impl Reason {
    /// How the wait in line ended, when it did; `None` when the slots were
    /// unusable.
    fn ended(&self) -> Option<&greenharness_slots::LineEnd> {
        match self {
            Reason::Ended(end) => Some(end),
            Reason::DirUnsafe(_) | Reason::Unsupported | Reason::Io(_) => None,
        }
    }
}

/// What a deferral of this run came from, for the roll-up: only an `onBusy
/// defer` `slotWait` is stopped by `set onBusy wait`; a ceiling or a failed
/// poll defers under either policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeferredBy {
    SlotWait,
    LineEnded,
}

/// Set once a check deferred because `onBusy defer`'s `slotWait` ran out.
static DEFERRED_BY_SLOT_WAIT: AtomicBool = AtomicBool::new(false);

/// What this run's deferrals came from.
pub fn deferred_by() -> DeferredBy {
    if DEFERRED_BY_SLOT_WAIT.load(Ordering::SeqCst) {
        DeferredBy::SlotWait
    } else {
        DeferredBy::LineEnded
    }
}

/// Whether this run already deferred on a busy host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostSeen {
    Busy,
    NotYet,
}

/// Set once a check of this process deferred: the host was measured busy,
/// and a later Heavy check of the same run defers without waiting again, so
/// a push waits at most one `slotWait`.
static BUSY_SEEN: AtomicBool = AtomicBool::new(false);

/// Both host sizing keys, `greengate.hostCores` and `greengate.hostSlots`,
/// resolved by the slots crate from the environment and this `Settings`'
/// cached rows: one implementation, shared with relais, and no extra git
/// spawn. Each warning is printed once.
pub fn host_sizing(settings: &crate::config::Settings) -> greenharness_slots::host::HostSizing {
    let rows = crate::config::host_rows(settings);
    let mut sizing = greenharness_slots::host::resolve(
        |name| std::env::var(name).ok(),
        rows.unwrap_or(&[]),
        greenharness_slots::detect_cores(),
    );
    if rows.is_none() {
        sizing
            .warnings
            .push(greenharness_slots::host::NO_SCOPES_WARNING.to_string());
    }
    for warning in &sizing.warnings {
        say_once(warning);
    }
    sizing
}

/// A sizing warning on stderr, once per process. Plain text: the warning
/// sign's colour follows stdout, and this line is on stderr.
fn say_once(warning: &str) {
    use std::collections::BTreeSet;
    use std::sync::{Mutex, OnceLock};
    static SAID: OnceLock<Mutex<BTreeSet<String>>> = OnceLock::new();
    let said = SAID.get_or_init(|| Mutex::new(BTreeSet::new()));
    let fresh = match said.lock() {
        Ok(mut set) => set.insert(warning.to_string()),
        Err(_) => true,
    };
    if fresh {
        eprintln!("{}: warning: {warning}", crate::names::BIN);
    }
}

/// The slot sizing and the machine it is computed for, from one resolution:
/// `greengate.hostSlots` (`None` when `0` turns slots off) and
/// `greengate.hostCores`, else the detected physical core count.
pub fn sizing_and_machine(
    settings: &crate::config::Settings,
) -> (
    Option<greenharness_slots::Sizing>,
    greenharness_slots::Machine,
) {
    let host = host_sizing(settings);
    (
        host.slots.value,
        greenharness_slots::Machine {
            cores: host.cores.value.max(1),
        },
    )
}

/// What turned `slot promote` off, or `None` when it is on: the environment
/// and `greengate.hostPromote`, resolved by the slots crate from the same
/// cached rows as the sizing keys. Its warnings print once per process.
pub fn promote_switch(
    settings: &crate::config::Settings,
) -> Option<greenharness_slots::host::OffBy> {
    let rows = crate::config::host_rows(settings);
    let (off, warnings) =
        greenharness_slots::host::resolve_promote(|name| std::env::var(name).ok(), rows);
    for warning in &warnings {
        say_once(warning);
    }
    off
}

/// `greengate.hostSlots`: unset sizes the slots from the machine
/// (`Sizing::Auto`), N fixes them, `0` turns them off (`None`).
pub fn sizing(settings: &crate::config::Settings) -> Option<greenharness_slots::Sizing> {
    sizing_and_machine(settings).0
}

/// The environment override of `greengate.onBusy`, for one run:
/// `GREENGATE_ON_BUSY=wait git push` runs a gate the file would defer.
pub const ON_BUSY_ENV: &str = "GREENGATE_ON_BUSY";

/// What a Heavy check does when no slot frees in time (ADR-0013,
/// `hooks.busy-policy`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusyPolicy {
    /// Wait in line, with no stall end, up to `greengate.timeout`, then defer
    /// the check to CI (ADR-0022, amended for the hook path): the default.
    Wait,
    /// Wait up to `slot_wait`, then defer the check to CI.
    Defer { slot_wait: Duration },
    /// Skip the queue: run unqueued at once, journalled `now` (ADR-0022).
    /// Only `GREENGATE_ON_BUSY=now` sets it, for one run: no committed file
    /// can make every clone skip the queue.
    Now,
}

/// The busy policy in force: `GREENGATE_ON_BUSY` (`wait`, `defer` or `now`),
/// then `greengate.onBusy` (git config, or `set onBusy` in a trusted
/// `amont.conf`; `wait` or `defer`), then `wait`.
pub fn busy_policy(settings: &crate::config::Settings) -> BusyPolicy {
    resolve_busy(std::env::var(ON_BUSY_ENV).ok().as_deref(), settings)
}

/// [`busy_policy`] with the environment value handed in: the testable seam,
/// because setting a variable in a threaded test binary races its siblings.
fn resolve_busy(from_env: Option<&str>, settings: &crate::config::Settings) -> BusyPolicy {
    let from_env = from_env.filter(|v| !v.is_empty());
    let mode = match from_env.map(str::to_ascii_lowercase).as_deref() {
        Some("wait") => "wait",
        Some("defer") => "defer",
        Some("now") => return BusyPolicy::Now,
        Some(other) => {
            crate::config::complain(
                ON_BUSY_ENV,
                &format!("{other:?} is not one of wait, defer, now"),
                "greengate.onBusy",
            );
            crate::config::enumerated_or(settings, "greengate.onBusy", &["wait", "defer"], "wait")
        }
        None => {
            crate::config::enumerated_or(settings, "greengate.onBusy", &["wait", "defer"], "wait")
        }
    };
    if mode == "defer" {
        let secs = crate::config::integer_or(settings, "greengate.slotWait", 60, 1..=3600) as u64;
        BusyPolicy::Defer {
            slot_wait: Duration::from_secs(secs),
        }
    } else {
        BusyPolicy::Wait
    }
}

/// What the check running on THIS thread is, as far as slots go.
#[derive(Default)]
struct Current {
    /// The check's name, for the messages its tools' runners print.
    name: String,
    heavy: bool,
    /// The slot question has been answered for this check (held, or
    /// unqueued for a reason).
    settled: bool,
    held: Option<greenharness_slots::Lease>,
    queued: Duration,
    /// Set when the busy policy deferred this check (ADR-0013): how long it
    /// waited. Read by the dispatcher through [`CheckGuard::deferred`], and it
    /// overrides whatever the check returned, so a check whose own error
    /// handling turns the refused spawn into `Unavailable` or `Failed` still
    /// ends Deferred.
    deferred: Option<Duration>,
    /// Why it deferred, for the refused spawn's error.
    deferred_cause: String,
    /// How many jobs this heavy check's tools may run
    /// (`greenharness_slots::parallelism`), held or unqueued; `None` when slots are
    /// off, unsupported, or nested under a holder whose bounds it inherits.
    jobs: Option<u32>,
    /// The (cap, cores) this check was sized for, carried to a handed slot's
    /// `waits.jsonl` line.
    sized: Option<(u32, u32)>,
    /// How this check waits for a slot.
    slot_wait: SlotWait,
}

/// How a heavy check waits for a slot.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum SlotWait {
    /// The busy policy in force.
    #[default]
    Policy,
    /// The check's severity cannot block the commit or push (`warn`): a
    /// finding it might make never stops anything, so it never waits for a
    /// slot, except inside a rehearsal snapshot (no open push connection, no
    /// waiting commit, and land's pre-stage needs the evidence). Set by
    /// [`never_waits`].
    NeverWaits,
    /// A heavy `warn` check of land's pre-stage: it waits, with no stall end,
    /// at most `greengate.warnSlotWait`, then defers (ADR-0022). Set by
    /// [`wait_bounded_for_warn`].
    BoundedForWarn,
}

/// A slot kept between two heavy gates of one rehearsal pre-push (ADR-0014):
/// the lease and the width its tools were given.
struct Parked {
    lease: greenharness_slots::Lease,
    jobs: Option<u32>,
    sized: Option<(u32, u32)>,
}

/// The slot parked between gates. Process-wide, because the next gate may
/// run on another thread than the one that finished; a `static` is never
/// dropped, so [`ParkScope`] releases it.
static PARKED: Mutex<Option<Parked>> = Mutex::new(None);

/// How many [`ParkScope`]s are alive: an inner scope's drop never stops
/// parking for an outer one.
static PARK_SCOPE_DEPTH: AtomicUsize = AtomicUsize::new(0);

fn parked() -> std::sync::MutexGuard<'static, Option<Parked>> {
    PARKED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Whether a finished heavy check parks its slot rather than release it: a
/// pre-push in a rehearsal snapshot, inside its [`ParkScope`]. The other
/// callers of [`enter_check`] release as they always did.
fn parking() -> bool {
    PARK_SCOPE_DEPTH.load(Ordering::SeqCst) > 0 && crate::rehearsal::in_snapshot()
}

/// Held by `dispatch::pre_push` around its gate loop. While it lives, a slot
/// a heavy gate held is handed to the next heavy gate instead of released;
/// when the outermost one drops, on any exit, the parked slot is released.
#[must_use = "the scope releases the parked slot when dropped"]
pub struct ParkScope {
    _private: (),
}

impl ParkScope {
    #[must_use = "the scope releases the parked slot when dropped"]
    pub fn begin() -> ParkScope {
        PARK_SCOPE_DEPTH.fetch_add(1, Ordering::SeqCst);
        ParkScope { _private: () }
    }
}

impl Drop for ParkScope {
    fn drop(&mut self) {
        if PARK_SCOPE_DEPTH.fetch_sub(1, Ordering::SeqCst) == 1 {
            release_parked();
        }
    }
}

/// Release the slot parked between gates, if any. A gate that runs on the
/// cluster executor takes no host slot (ADR-0016), so it calls this where it
/// claims its Job: a slot parked for the next local gate would sit idle for
/// the whole cluster run.
pub fn release_parked() {
    let leftover = parked().take();
    drop(leftover);
}

/// The holder record of a slot parked between gates.
fn between_gates() -> greenharness_slots::Holder {
    holder_for("pre-push, between checks")
}

/// amont's holder record for `command`. A push rehearsal runs in a temporary
/// snapshot: the record names the worktree it was taken from, which is what a
/// reader of `amont slots` knows, whether the slot was granted, parked or
/// handed on.
fn holder_for(command: &str) -> greenharness_slots::Holder {
    let mut holder = greenharness_slots::Holder::here(crate::names::SLOT_HOLDER, command);
    if let Some(src) = crate::rehearsal::source_worktree() {
        holder.repo = src.display().to_string();
    }
    holder
}

thread_local! {
    static CURRENT: RefCell<Current> = RefCell::new(Current::default());
}

/// Installed by the dispatcher around one check's `run`; clears this
/// thread's slot state, and releases a held slot, when dropped.
pub struct CheckGuard;

impl CheckGuard {
    /// How long this check waited for a slot so far.
    pub fn queued(&self) -> Duration {
        CURRENT.with(|c| c.borrow().queued)
    }

    /// Whether the busy policy deferred this check, and after how long a
    /// wait. Read before the guard drops.
    pub fn deferred(&self) -> Option<Duration> {
        CURRENT.with(|c| c.borrow().deferred)
    }
}

/// Whether the check running on this thread was deferred by the busy
/// policy: what it says after that is about its refused spawn.
pub fn deferred_here() -> bool {
    CURRENT.with(|c| c.borrow().deferred.is_some())
}

/// The error a refused spawn returns, naming why the check deferred. Only a
/// way to stop the spawn: what the check makes of it does not matter, because
/// [`CheckGuard::deferred`] wins.
fn refusal() -> std::io::Error {
    let cause = CURRENT.with(|c| c.borrow().deferred_cause.clone());
    std::io::Error::other(if cause.is_empty() {
        "deferred to CI: the host is busy".to_string()
    } else {
        cause
    })
}

/// Why a wait that ended in line defers, as the refused spawn says it.
fn defer_cause(
    end: &greenharness_slots::LineEnd,
    policy: BusyPolicy,
    slot_wait: SlotWait,
) -> String {
    use greenharness_slots::LineEnd;
    let human = |d: Duration| crate::hooks::common::human_secs(d.as_secs());
    match end {
        // The hook path sets no stall window, so this arm fires only if a
        // caller passes one.
        LineEnd::Stalled { window, position } => format!(
            "deferred to CI: the host slot line made no progress for {} (position {})",
            human(*window),
            position + 1
        ),
        LineEnd::PollFailed { error } => {
            format!("deferred to CI: the host slot files failed: {error}")
        }
        LineEnd::Ceiling { waited } => match policy {
            BusyPolicy::Defer { .. } => format!(
                "deferred to CI: no host slot freed in {} (greengate.onBusy defer)",
                human(*waited)
            ),
            BusyPolicy::Wait | BusyPolicy::Now => {
                // `NeverWaits` always arrives as `Defer`; it is listed so the
                // match covers every case.
                let setting = match slot_wait {
                    SlotWait::BoundedForWarn => "greengate.warnSlotWait",
                    SlotWait::Policy | SlotWait::NeverWaits => "greengate.timeout",
                };
                format!(
                    "deferred to CI: no host slot within {} ({setting})",
                    human(*waited)
                )
            }
        },
    }
}

/// How one heavy check waits for a slot (ADR-0022): it skips the queue, or
/// it waits in line within bounds. Whatever ends the wait, the check defers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WaitPlan {
    Now,
    Queue(greenharness_slots::WaitBounds),
}

/// The [`WaitPlan`] for a check: one row of the plan's table per arm.
/// `ceiling` is `greengate.timeout` (3600 s when 0), `seen` whether this run
/// already deferred on a busy host. A hook check has no stall window: it waits
/// until its ceiling, as a commit or a push can, and defers there (ADR-0022,
/// amended for the hook path in 5.1.1); the stall end belongs to `slot run`
/// and relais.
fn wait_plan(
    slot_wait: SlotWait,
    policy: BusyPolicy,
    ceiling: Duration,
    warn_wait: Duration,
    seen: HostSeen,
) -> WaitPlan {
    let queue =
        |window, ceiling| WaitPlan::Queue(greenharness_slots::WaitBounds { window, ceiling });
    match (slot_wait, policy) {
        (_, BusyPolicy::Now) => WaitPlan::Now,
        (SlotWait::NeverWaits, _) => queue(None, Duration::ZERO),
        (SlotWait::BoundedForWarn, _) => queue(None, warn_wait.min(ceiling)),
        (SlotWait::Policy, BusyPolicy::Wait) => queue(None, ceiling),
        (SlotWait::Policy, BusyPolicy::Defer { slot_wait }) => match seen {
            // The host was already measured busy by this run: defer now.
            HostSeen::Busy => queue(None, Duration::ZERO),
            HostSeen::NotYet => queue(None, slot_wait),
        },
    }
}

/// The bounds of the second wait an `onBusy defer` check takes after its
/// `slotWait` ran out: while a sibling check of this process holds a slot,
/// the host is busy with this commit's own work, which is not a reason to
/// defer, so it waits as `wait` does, with no stall window, up to the
/// ceiling; else one more pass, as a sibling may have released between the
/// last poll and now.
fn sibling_bounds(
    slot_wait: SlotWait,
    held_in_process: usize,
    ceiling: Duration,
    elapsed: Duration,
) -> greenharness_slots::WaitBounds {
    let waits_on_sibling = held_in_process > 0 && !matches!(slot_wait, SlotWait::NeverWaits);
    if waits_on_sibling {
        greenharness_slots::WaitBounds {
            window: None,
            ceiling: ceiling.saturating_sub(elapsed),
        }
    } else {
        greenharness_slots::WaitBounds {
            window: None,
            ceiling: Duration::ZERO,
        }
    }
}

impl Drop for CheckGuard {
    fn drop(&mut self) {
        let finished = CURRENT.with(RefCell::take);
        if let Some(lease) = finished.held {
            if parking() {
                park(lease, finished.jobs, finished.sized);
            }
        }
    }
}

/// Keep `lease` for the next heavy check of this pre-push, its record
/// renamed so no waiter's line names the gate that finished.
fn park(lease: greenharness_slots::Lease, jobs: Option<u32>, sized: Option<(u32, u32)>) {
    if let Err(error) = lease.rename(&between_gates()) {
        crate::hooks::common::say(&format!(
            "  host slot kept, but `{BIN} slots` may still name the previous check: {error}"
        ));
    }
    let earlier = parked().replace(Parked { lease, jobs, sized });
    drop(earlier);
}

/// The dispatcher is about to run `name`, of `weight`, on this thread. The
/// weight comes from the check (`Check::weight`), not the name: a declared
/// check is Heavy by its line, which no name says.
pub fn enter_check(name: &str, weight: Weight) -> CheckGuard {
    CURRENT.with(|c| {
        *c.borrow_mut() = Current {
            name: name.to_string(),
            heavy: weight == Weight::Heavy,
            ..Current::default()
        }
    });
    CheckGuard
}

/// The check running on this thread cannot block (its severity is `warn`):
/// on a busy host it takes a free slot or defers to CI at once, under either
/// busy policy, instead of holding the commit, or the push and its open
/// connection to the remote, while it queues for a verdict that stops
/// nothing. Not called inside a rehearsal snapshot: there nothing is held
/// open and land's pre-stage needs the gate's evidence, so the check follows
/// the busy policy the rehearsal sets.
pub fn never_waits() {
    CURRENT.with(|current| current.borrow_mut().slot_wait = SlotWait::NeverWaits);
}

/// The check running on this thread is a heavy `warn` check of land's
/// pre-stage, where somebody waits for its evidence: it waits for a slot at
/// most `greengate.warnSlotWait` (and never longer than `greengate.timeout`), under
/// either busy policy, with no stall end, then defers (ADR-0022).
pub fn wait_bounded_for_warn() {
    CURRENT.with(|current| current.borrow_mut().slot_wait = SlotWait::BoundedForWarn);
}

/// `greengate.warnSlotWait`: how long a heavy `warn` check of land's pre-stage
/// waits for a slot. Git config, then a trusted `amont.conf`, then 600 s.
fn warn_slot_wait(settings: &crate::config::Settings) -> Duration {
    Duration::from_secs(
        crate::config::integer_or(settings, "greengate.warnSlotWait", 600, 1..=3600) as u64,
    )
}

/// The name of the check running on this thread, without its stage
/// prefix (`clippy`), when the dispatcher installed one.
pub fn current_check() -> Option<String> {
    CURRENT.with(|c| {
        let c = c.borrow();
        (!c.name.is_empty()).then(|| crate::short_name(&c.name).to_string())
    })
}

/// The held marker of this run, read once on `settings` (t15): every check
/// and thread of one run gets the same answer, so a parent lease ending
/// midway cannot admit some checks and queue others.
fn marker(settings: &crate::config::Settings) -> &greenharness_slots::Marker {
    settings.marker.get_or_init(greenharness_slots::held_marker)
}

/// Whether an ancestor holds a slot for this run: its `GREENHARNESS_HOST_SLOT`
/// names a live lease (`greenharness_slots::held_marker`).
pub(crate) fn nested(settings: &crate::config::Settings) -> bool {
    marker(settings).is_held()
}

/// For a command about to queue: when its marker was refused, the journal's
/// `marker_refused` row and the lines to print, the first time only.
pub(crate) fn marker_refused_note(settings: &crate::config::Settings) -> Option<String> {
    let marker = marker(settings);
    settings
        .marker_log
        .record(
            &greenharness_slots::dir(),
            crate::names::SLOT_HOLDER,
            marker,
        )
        .then(|| greenharness_slots::refused_line(BIN, marker))
        .flatten()
}

/// Print a refusal's lines: into the check's output when a stage is
/// watching it, else on stderr, never on stdout (which a caller may read as
/// data).
fn say_refusal(line: &str) {
    if crate::live::current_sink().is_some() {
        crate::hooks::common::say(line);
    } else {
        eprintln!("{line}");
    }
}

/// Called before a tool is spawned. For the first tool of a heavy check,
/// waits for a slot, showing the wait in the check's row. Returns whether
/// this thread holds a slot, so the caller exports [`HELD_ENV`]; an `Err`
/// when the busy policy deferred the check, so the caller does not spawn.
pub fn before_spawn(settings: &crate::config::Settings) -> std::io::Result<bool> {
    let (pending, deferred, name, slot_wait) = CURRENT.with(|current| {
        let current = current.borrow();
        (
            current.heavy && !current.settled,
            current.deferred.is_some(),
            current.name.clone(),
            current.slot_wait,
        )
    });
    // A check that caught the first refusal and tries another tool is
    // refused at once, not after a second wait.
    if deferred {
        return Err(refusal());
    }
    if pending {
        if let Some(handed) = parking().then(|| parked().take()).flatten() {
            return Ok(take_handed(handed, &name));
        }
        // Nested first: a nested amont must not pay the host-key scan for a
        // slot count it is about to ignore. Neither answer queues, so neither
        // can defer.
        let (sizing, machine) = if nested(settings) {
            (None, greenharness_slots::Machine::detect())
        } else {
            sizing_and_machine(settings)
        };
        let Some(sizing) = sizing else {
            CURRENT.with(|c| c.borrow_mut().settled = true);
            return Ok(false);
        };
        if !greenharness_slots::SUPPORTED {
            CURRENT.with(|c| c.borrow_mut().settled = true);
            return Ok(false);
        }
        let policy = match slot_wait {
            SlotWait::NeverWaits => BusyPolicy::Defer {
                slot_wait: Duration::ZERO,
            },
            SlotWait::BoundedForWarn => BusyPolicy::Wait,
            SlotWait::Policy => busy_policy(settings),
        };
        let ceiling = Duration::from_secs(match crate::hooks::common::check_timeout(settings) {
            0 => 3600,
            s => s,
        });
        let seen = if BUSY_SEEN.load(Ordering::SeqCst) {
            HostSeen::Busy
        } else {
            HostSeen::NotYet
        };
        let plan = wait_plan(slot_wait, policy, ceiling, warn_slot_wait(settings), seen);
        let slot_cap = sizing.cap_for(machine.cores);
        let cap = u64::from(slot_cap);
        // Hook checks take one slot; the width is set whether a slot is
        // granted or the check runs unqueued.
        let jobs = greenharness_slots::parallelism(1, machine.cores, slot_cap);
        let bounds = match plan {
            WaitPlan::Queue(bounds) => bounds,
            WaitPlan::Now => {
                // Skipping the queue is a choice made up front (ADR-0022),
                // said and journalled as such.
                crate::hooks::common::say(&format!(
                    "  running unqueued: {}=now skips the host slot queue",
                    crate::ui::highlight(ON_BUSY_ENV)
                ));
                record_acquisition(
                    &name,
                    Some(jobs),
                    Some((slot_cap, machine.cores)),
                    Duration::ZERO,
                    "now",
                    None,
                );
                CURRENT.with(|c| {
                    let mut c = c.borrow_mut();
                    c.settled = true;
                    c.jobs = Some(jobs);
                    c.sized = Some((slot_cap, machine.cores));
                });
                return Ok(false);
            }
        };
        // A marker that names no live lease queues like none, said once.
        if let Some(line) = marker_refused_note(settings) {
            say_refusal(&line);
        }
        let sink = crate::live::current_sink();
        if let Some((stage, idx)) = &sink {
            stage.queue(*idx, Some((Instant::now(), cap)));
        }
        let started = Instant::now();
        let holder = holder_for(&name);
        let mut got = wait_for_slot(sizing, machine, holder.clone(), bounds);
        if let (
            BusyPolicy::Defer { .. },
            Err(Reason::Ended(greenharness_slots::LineEnd::Ceiling { .. })),
        ) = (policy, &got)
        {
            let more = sibling_bounds(
                slot_wait,
                greenharness_slots::held_in_process(),
                ceiling,
                started.elapsed(),
            );
            got = wait_for_slot(sizing, machine, holder, more);
        }
        if let Some((stage, idx)) = &sink {
            stage.queue(*idx, None);
        }
        let waited = started.elapsed();
        CURRENT.with(|c| c.borrow_mut().queued += waited);
        // Piped output never shows the region, and a heartbeat only comes
        // after a minute: say once where the time went.
        if got.is_ok() && waited >= Duration::from_secs(1) {
            crate::hooks::common::say(&format!(
                "  (waited {} for a host slot, {} {cap})",
                crate::hooks::common::human_secs(waited.as_secs()),
                crate::ui::highlight("greengate.hostSlots")
            ));
        }
        // A wait that ended in line defers, whatever the policy: nothing
        // runs outside the queue because a wait ended (ADR-0022).
        let ended = got.as_ref().err().and_then(Reason::ended);
        let defer = ended.is_some();
        let end = ended.map(greenharness_slots::LineEnd::end);
        let cause = ended.map_or(String::new(), |end| defer_cause(end, policy, slot_wait));
        let by_slot_wait = matches!(
            (policy, ended),
            (
                BusyPolicy::Defer { .. },
                Some(greenharness_slots::LineEnd::Ceiling { .. })
            )
        );
        let outcome = match (&got, defer) {
            (Ok(_), _) => "admitted",
            (Err(_), true) => "deferred",
            (Err(_), false) => "unqueued",
        };
        record_acquisition(
            &name,
            Some(jobs),
            Some((slot_cap, machine.cores)),
            waited,
            outcome,
            end,
        );
        let held = match got {
            Ok(lease) => Some(lease),
            // The deferral line is the dispatcher's, not this note.
            Err(Reason::Ended(_)) => None,
            Err(reason) => {
                note(&reason);
                None
            }
        };
        if defer {
            BUSY_SEEN.store(true, Ordering::SeqCst);
            if by_slot_wait {
                DEFERRED_BY_SLOT_WAIT.store(true, Ordering::SeqCst);
            }
        }
        CURRENT.with(|c| {
            let mut c = c.borrow_mut();
            c.held = held;
            c.settled = true;
            c.jobs = Some(jobs);
            c.sized = Some((slot_cap, machine.cores));
            if defer {
                c.deferred = Some(waited);
                c.deferred_cause = cause;
            }
        });
        if defer {
            return Err(refusal());
        }
    }
    // A light check run while a slot is parked between two heavy gates: its
    // tools run under that slot (a child `amont slot run` must not queue for
    // its parent's slot).
    Ok(CURRENT.with(|c| c.borrow().held.is_some()) || (parking() && parked().is_some()))
}

/// The next heavy check takes the slot the previous one parked: renamed to
/// itself, recorded as a wait of nothing, its width applied as a grant's is.
fn take_handed(handed: Parked, name: &str) -> bool {
    let Parked { lease, jobs, sized } = handed;
    if let Err(error) = lease.rename(&holder_for(name)) {
        crate::hooks::common::say(&format!(
            "  host slot kept, but `{BIN} slots` may still name the previous check: {error}"
        ));
    }
    record_acquisition(name, jobs, sized, Duration::ZERO, "handed", None);
    CURRENT.with(|current| {
        let mut current = current.borrow_mut();
        current.held = Some(lease);
        current.settled = true;
        current.jobs = jobs;
        current.sized = sized;
    });
    true
}

/// One line of `waits.jsonl` for the check `name`, with where it ran.
/// `sized` is the (cap, cores) the waiter was sized for, when known.
fn record_acquisition(
    name: &str,
    jobs: Option<u32>,
    sized: Option<(u32, u32)>,
    waited: Duration,
    outcome: &str,
    end: Option<greenharness_slots::WaitEnd>,
) {
    let short = crate::short_name(name);
    greenharness_slots::record_wait_named(
        &greenharness_slots::dir(),
        crate::names::SLOT_HOLDER,
        greenharness_slots::WaitContext {
            name: Some(short),
            place: if crate::rehearsal::in_prestage() {
                greenharness_slots::WaitPlace::Prestage
            } else if crate::rehearsal::in_snapshot() {
                greenharness_slots::WaitPlace::Rehearsal
            } else {
                greenharness_slots::WaitPlace::Push
            },
            cap: sized.map(|(cap, _)| cap),
            cores: sized.map(|(_, cores)| cores),
            end,
        },
        1,
        jobs,
        waited.as_millis(),
        outcome,
    );
}

/// Bound `cmd` to the width this thread's heavy check was given (see
/// `Current::jobs`); a light check, or slots off, sets nothing. Called by
/// every runner after [`before_spawn`].
pub fn bound(cmd: &mut std::process::Command) {
    let own = CURRENT.with(|c| c.borrow().jobs);
    // A light check between two heavy gates runs under the parked slot.
    if let Some(jobs) = own.or_else(|| {
        parking()
            .then(|| parked().as_ref().and_then(|p| p.jobs))
            .flatten()
    }) {
        greenharness_slots::bound_width(cmd, jobs);
    }
}

/// Wait in line for one slot within `bounds`, saying once whom the check
/// waits behind (ADR-0022: the crate's [`greenharness_slots::wait_in_line`]).
/// Failing to join the queue at all says the slots are unusable.
fn wait_for_slot(
    sizing: greenharness_slots::Sizing,
    machine: greenharness_slots::Machine,
    holder: greenharness_slots::Holder,
    bounds: greenharness_slots::WaitBounds,
) -> Result<greenharness_slots::Lease, Reason> {
    let waiter = match greenharness_slots::Waiter::new_in_on(
        &greenharness_slots::dir(),
        sizing,
        1,
        holder,
        machine,
    ) {
        Ok(w) => w,
        Err(e) if e.kind() == std::io::ErrorKind::Other => {
            return Err(Reason::DirUnsafe(e.to_string()))
        }
        Err(e) => return Err(Reason::Io(e.to_string())),
    };
    let mut said = false;
    greenharness_slots::wait_in_line(waiter, bounds, |queue| {
        if !said && (!queue.holders.is_empty() || queue.unknown > 0) {
            said = true;
            crate::hooks::common::say(&format!("  {}", behind(queue)));
            crate::hooks::common::say(&format!("  see: {BIN} slots"));
        }
    })
    .map_err(Reason::Ended)
}

/// `queued behind relais: cargo test (greengate-wt-x, 4m) · 2 waiting`.
pub fn behind(queue: &greenharness_slots::Queue) -> String {
    let now = greenharness_slots::now_ms();
    let mut who: Vec<String> = queue.holders.iter().map(|h| h.describe(now)).collect();
    if queue.unknown > 0 {
        who.push(format!("{} unknown (old client)", queue.unknown));
    }
    let reason = queue
        .reason
        .map_or(String::new(), |r| format!(" ({})", r.describe()));
    format!(
        "queued behind {}{reason} · {} waiting",
        who.join(", "),
        queue.waiting.max(1)
    )
}

/// One line for the reasons a person can act on; nothing for the normal
/// case (the platform). A wait that ended in line defers, and the dispatcher
/// says so; these are the slots being unusable.
fn note(reason: &Reason) {
    match reason {
        Reason::DirUnsafe(why) => crate::hooks::common::warn(&format!(
            "host slots are off for this check: {why}. Remove it, or set {} to a directory \
             only you own",
            crate::ui::highlight(DIR_ENV)
        )),
        Reason::Io(why) => {
            crate::hooks::common::warn(&format!("host slots are off for this check: {why}"))
        }
        Reason::Ended(_) | Reason::Unsupported => {}
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn settings_from(conf: &str) -> crate::config::Settings {
        let lines = crate::manifest::parse_lines(conf);
        let (policy, _) = crate::policy::Policy::from_lines(&lines);
        crate::config::Settings::new(policy)
    }

    /// `GREENGATE_ON_BUSY`, then `amont.conf`, then `wait`; a bad environment
    /// value is ignored for the file's.
    #[test]
    fn the_busy_policy_is_env_then_file_then_wait() {
        let none = settings_from("");
        let defer = settings_from("set onBusy defer\nset slotWait 5\n");
        assert_eq!(resolve_busy(None, &none), BusyPolicy::Wait);
        assert_eq!(
            resolve_busy(None, &defer),
            BusyPolicy::Defer {
                slot_wait: Duration::from_secs(5)
            }
        );
        assert_eq!(resolve_busy(Some("wait"), &defer), BusyPolicy::Wait);
        assert_eq!(
            resolve_busy(Some("DEFER"), &none),
            BusyPolicy::Defer {
                slot_wait: Duration::from_secs(60)
            }
        );
        assert_eq!(
            resolve_busy(Some("later"), &defer),
            BusyPolicy::Defer {
                slot_wait: Duration::from_secs(5)
            }
        );
        assert_eq!(resolve_busy(Some(""), &none), BusyPolicy::Wait);
        // ADR-0022: `now` skips the queue for one run, from the environment
        // only; the file and git config keep `wait|defer`.
        assert_eq!(resolve_busy(Some("now"), &defer), BusyPolicy::Now);
        let committed = settings_from("set onBusy now\n");
        assert_eq!(resolve_busy(None, &committed), BusyPolicy::Wait);
    }

    /// ADR-0022, one row of the plan's table per assertion: skip the
    /// queue, or wait in line within these bounds.
    #[test]
    fn each_caller_waits_by_its_row_of_the_table() {
        use greenharness_slots::WaitBounds;
        let s = |n| Duration::from_secs(n);
        let (ceiling, warn) = (s(3600), s(600));
        let defer = BusyPolicy::Defer { slot_wait: s(60) };
        let queue = |window, ceiling| WaitPlan::Queue(WaitBounds { window, ceiling });
        let row =
            |slot_wait, policy, ceiling, seen| wait_plan(slot_wait, policy, ceiling, warn, seen);
        // `onBusy wait`, the default (rehearsal and pre-stage too): no stall
        // window on the hook path, only the ceiling.
        assert_eq!(
            row(
                SlotWait::Policy,
                BusyPolicy::Wait,
                ceiling,
                HostSeen::NotYet
            ),
            queue(None, ceiling)
        );
        // A `warn` check of land's pre-stage: no stall window, under
        // warnSlotWait, which never outlasts the ceiling.
        assert_eq!(
            row(
                SlotWait::BoundedForWarn,
                BusyPolicy::Wait,
                ceiling,
                HostSeen::NotYet
            ),
            queue(None, warn)
        );
        assert_eq!(
            row(
                SlotWait::BoundedForWarn,
                BusyPolicy::Wait,
                s(100),
                HostSeen::NotYet
            ),
            queue(None, s(100))
        );
        // `onBusy defer`: slotWait stays hard, with no stall rule; none at
        // all once this run measured the host busy.
        assert_eq!(
            row(SlotWait::Policy, defer, ceiling, HostSeen::NotYet),
            queue(None, s(60))
        );
        assert_eq!(
            row(SlotWait::Policy, defer, ceiling, HostSeen::Busy),
            queue(None, Duration::ZERO)
        );
        // A `warn` check outside a rehearsal never waits.
        assert_eq!(
            row(
                SlotWait::NeverWaits,
                BusyPolicy::Wait,
                ceiling,
                HostSeen::NotYet
            ),
            queue(None, Duration::ZERO)
        );
        // `GREENGATE_ON_BUSY=now`: no wait at all.
        assert_eq!(
            row(SlotWait::Policy, BusyPolicy::Now, ceiling, HostSeen::NotYet),
            WaitPlan::Now
        );
    }

    /// The sibling-holder extension: with a sibling of this process holding
    /// a slot, the second wait goes on, with no stall window, to the rest of
    /// the ceiling; without one it is a single last poll.
    #[test]
    fn the_sibling_extension_waits_to_the_ceiling() {
        let s = |n| Duration::from_secs(n);
        let with_sibling = sibling_bounds(SlotWait::Policy, 1, s(3600), s(60));
        assert_eq!(
            with_sibling.window, None,
            "no stall window on the hook path"
        );
        assert_eq!(with_sibling.ceiling, s(3540));
        let alone = sibling_bounds(SlotWait::Policy, 0, s(3600), s(60));
        assert_eq!((alone.window, alone.ceiling), (None, Duration::ZERO));
        let never = sibling_bounds(SlotWait::NeverWaits, 1, s(3600), s(60));
        assert_eq!((never.window, never.ceiling), (None, Duration::ZERO));
    }

    /// The refused spawn names why: a stall names its own window (the
    /// sibling extension's too) and the place; `slotWait` keeps naming
    /// `onBusy defer`; a failed poll names the error.
    #[test]
    fn a_deferral_names_its_cause() {
        use greenharness_slots::LineEnd;
        let stalled = LineEnd::Stalled {
            window: Duration::from_secs(300),
            position: 1,
        };
        let cause = defer_cause(
            &stalled,
            BusyPolicy::Defer {
                slot_wait: Duration::from_secs(60),
            },
            SlotWait::Policy,
        );
        assert!(cause.contains("made no progress for 5m00s"), "{cause}");
        assert!(cause.contains("(position 2)"), "{cause}");
        let slot_wait = defer_cause(
            &LineEnd::Ceiling {
                waited: Duration::from_secs(60),
            },
            BusyPolicy::Defer {
                slot_wait: Duration::from_secs(60),
            },
            SlotWait::Policy,
        );
        assert!(
            slot_wait.ends_with("(greengate.onBusy defer)"),
            "{slot_wait}"
        );
        let ceiling = defer_cause(
            &LineEnd::Ceiling {
                waited: Duration::from_secs(3600),
            },
            BusyPolicy::Wait,
            SlotWait::Policy,
        );
        assert!(ceiling.contains("no host slot within 1h00m"), "{ceiling}");
        assert!(ceiling.ends_with("(greengate.timeout)"), "{ceiling}");
        let warn = defer_cause(
            &LineEnd::Ceiling {
                waited: Duration::from_secs(600),
            },
            BusyPolicy::Wait,
            SlotWait::BoundedForWarn,
        );
        assert!(warn.ends_with("(greengate.warnSlotWait)"), "{warn}");
        let failed = defer_cause(
            &LineEnd::PollFailed {
                error: std::io::Error::other("busy disk"),
            },
            BusyPolicy::Wait,
            SlotWait::Policy,
        );
        assert!(failed.contains("busy disk"), "{failed}");
    }

    /// Every end of a wait in line defers; only failing to join the queue
    /// means the slots are unusable.
    #[test]
    fn every_end_of_a_wait_in_line_defers() {
        let ended = Reason::Ended(greenharness_slots::LineEnd::Ceiling {
            waited: Duration::from_secs(1),
        });
        assert!(ended.ended().is_some());
        for reason in [
            Reason::Io("x".into()),
            Reason::DirUnsafe("x".into()),
            Reason::Unsupported,
        ] {
            assert!(reason.ended().is_none(), "{reason:?}");
        }
    }

    #[test]
    fn heavy_is_the_list() {
        assert_eq!(weight_of("pre-commit-clippy"), Weight::Heavy);
        assert_eq!(weight_of("pre-commit-cargo-fmt"), Weight::Light);
    }

    /// Every heavy name is a built-in: a renamed check must not silently
    /// stop queueing.
    #[test]
    fn every_heavy_name_is_a_builtin() {
        for name in HEAVY {
            assert!(
                crate::registry::CHECKS.iter().any(|c| c.name == *name),
                "{name} is not a built-in check"
            );
        }
    }

    /// `.cargo/config.toml` reaches the test binaries: without it the
    /// integration fixtures would queue on the real host slots. Slots are
    /// off, not a bare held marker, which the reader refuses once the
    /// rollout ends (host-slot-admission t15); the marker is forced empty so
    /// a real holder's `held:<lease>` does not leak in.
    #[test]
    fn cargo_pins_the_host_knobs_for_tests() {
        assert_eq!(std::env::var(HELD_ENV).as_deref(), Ok(""));
        assert_eq!(
            std::env::var(greenharness_slots::host::SLOTS_ENV).as_deref(),
            Ok("0")
        );
        assert_eq!(
            std::env::var("GREENGATE_IDLE_LOAD_SCALE").as_deref(),
            Ok("1")
        );
        assert_eq!(
            std::env::var(greenharness_slots::host::PROMOTE_ENV).as_deref(),
            Ok("1")
        );
    }

    /// A light check never asks. A heavy one with slots off (the test
    /// process has `GREENGATE_HOST_SLOTS=0` from `.cargo/config.toml`)
    /// settles without a slot.
    #[test]
    fn a_light_check_never_takes_a_slot() {
        let settings = crate::config::Settings::default();
        let _g = enter_check("pre-commit-cargo-fmt", weight_of("pre-commit-cargo-fmt"));
        assert!(!before_spawn(&settings).unwrap());
    }

    #[test]
    fn a_heavy_check_with_slots_off_settles_without_a_slot() {
        let settings = crate::config::Settings::default();
        let _g = enter_check("pre-commit-clippy", weight_of("pre-commit-clippy"));
        assert_eq!(current_check().as_deref(), Some("clippy"));
        assert!(!before_spawn(&settings).unwrap());
        assert!(CURRENT.with(|c| c.borrow().settled));
        // No slot: no width is set here.
        let mut cmd = std::process::Command::new("true");
        bound(&mut cmd);
        assert_eq!(cmd.get_envs().count(), 0);
    }

    /// A scope dropped on an early return releases the parked slot: nothing
    /// stays held, and no holder record stays on disk.
    #[test]
    fn a_park_scope_dropped_early_releases_the_parked_slot() {
        let dir = std::env::temp_dir().join(format!("greengate-park-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir); // best-effort: a fresh scratch dir
        let early = || {
            let _scope = ParkScope::begin();
            let mut waiter = greenharness_slots::Waiter::new_in(
                &dir,
                greenharness_slots::Sizing::Fixed(1),
                1,
                greenharness_slots::Holder::here(crate::names::SLOT_HOLDER, "gate"),
            )
            .unwrap();
            let greenharness_slots::Poll::Granted(lease) = waiter.poll().unwrap() else {
                panic!("the empty directory grants the slot");
            };
            assert!(dir.join("holders/slot-0.who").exists());
            park(lease, Some(2), Some((3, 6)));
            assert!(dir.join("holders/slot-0.who").exists());
        };
        early();
        assert!(!dir.join("holders/slot-0.who").exists());
        let _ = std::fs::remove_dir_all(&dir); // best-effort: scratch
    }

    #[test]
    fn a_light_check_sets_no_width() {
        let settings = crate::config::Settings::default();
        let _g = enter_check("pre-commit-cargo-fmt", weight_of("pre-commit-cargo-fmt"));
        before_spawn(&settings).unwrap();
        let mut cmd = std::process::Command::new("true");
        bound(&mut cmd);
        assert_eq!(cmd.get_envs().count(), 0);
    }
}
