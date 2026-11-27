//! The held marker (amont host-slot-admission t10, t15): what
//! [`HELD_ENV`](crate::HELD_ENV) must say before a process skips the queue
//! for a slot its ancestor holds.
//!
//! A real holder exports `held:<lease>`, its [`Lease`](crate::Lease) id
//! (`<pid>-<seq>`), and a child inherits it unchanged. This module is the one
//! reader both greengate and relais call. It answers [`Marker::Held`] only
//! when a holder record names that lease, its writer is alive, and its slot is
//! locked. Anything else is [`Marker::Refused`]: the command queues like an
//! unmarked one, with one stderr line ([`refused_line`]) and one journal row
//! ([`MarkerLog::record`]).
//!
//! The check stops careless and habitual use, not a determined agent: the
//! holder records are readable, so a live lease id can be copied. Nothing
//! secret is added, deliberately (t10's threat model).

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::{append_journal, escape, now_ms, platform, raw, session_id, Holder};

/// The prefix of the marker a holder exports: `held:<lease>`.
pub const MARKER_PREFIX: &str = "held:";

/// The marker a crate before [`LEASE_WRITER_SINCE`] exports: no lease named.
const BARE: &str = "held";

/// The first greengate and relais release whose writers export
/// `held:<lease>`. Before it, a holder exports bare `held`, which the reader
/// accepts until [`ACCEPT_BARE`] is switched off.
pub(crate) const LEASE_WRITER_SINCE: &str = "5.5";

/// Whether a bare `held` is still accepted. True for the rollout's first two
/// steps (every reader accepts both forms, then every writer emits the new
/// one); false once both tools are installed with the new writer (t15
/// phase 3).
const ACCEPT_BARE: BarePolicy = BarePolicy::Accept;

/// What the reader does with the bare `held`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BarePolicy {
    Accept,
    /// Built by the phase-3 switch; until then only the tests build it.
    #[allow(dead_code)]
    Refuse,
}

/// How much of a marker given a line or a row repeats.
const SHOWN_CHARS: usize = 64;

/// What [`HELD_ENV`](crate::HELD_ENV) says about this process.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Marker {
    /// Unset or empty: this process holds nothing, and queues.
    Absent,
    /// An ancestor holds a slot: no second queue. `bare` when the given was
    /// the old bare `held`, accepted during the rollout.
    Held { bare: bool },
    /// Set, but naming no live lease: this process queues.
    Refused { given: String, reason: Refusal },
}

impl Marker {
    /// Whether an ancestor holds a slot for this process.
    pub fn is_held(&self) -> bool {
        matches!(self, Marker::Held { .. })
    }
}

/// Why a marker was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Refusal {
    /// The bare `held`, once it is no longer accepted.
    Bare,
    /// Neither `held` nor `held:<pid>-<seq>`: `1`, a path, any other text.
    Malformed,
    /// No holder record names the lease.
    UnknownLease,
    /// A record names it, but its writer is gone.
    DeadPid,
    /// A live record names it, but no slot of the lease is locked.
    SlotFree,
    /// The slot directory could not be read, or is not this user's.
    Io,
}

impl Refusal {
    /// The journal word: `bare`, `malformed`, `unknown_lease`, `dead_pid`,
    /// `slot_free`, `io`. A refusal naming a lease in the records (`dead_pid`,
    /// `slot_free`) is a real child failing the check; the others are a forged
    /// or stale marker.
    pub fn as_str(self) -> &'static str {
        match self {
            Refusal::Bare => "bare",
            Refusal::Malformed => "malformed",
            Refusal::UnknownLease => "unknown_lease",
            Refusal::DeadPid => "dead_pid",
            Refusal::SlotFree => "slot_free",
            Refusal::Io => "io",
        }
    }
}

/// `held:<id>`: the marker a holder of the lease `id` exports.
pub fn held_value(id: &str) -> String {
    format!("{MARKER_PREFIX}{id}")
}

/// This process's marker, read from [`HELD_ENV`](crate::HELD_ENV) against
/// [`dir`](crate::dir). Reads the slot files every call: a caller that asks
/// more than once keeps the first answer.
pub fn held_marker() -> Marker {
    held_marker_in(&crate::dir(), std::env::var_os(crate::HELD_ENV).as_deref())
}

/// [`held_marker`] for a given directory and given: the seam tests use.
pub fn held_marker_in(dir: &Path, given: Option<&OsStr>) -> Marker {
    read_marker(dir, given, ACCEPT_BARE)
}

/// The reader, with the bare-marker policy as a parameter so tests cover
/// both steps of the rollout.
fn read_marker(dir: &Path, given: Option<&OsStr>, bare: BarePolicy) -> Marker {
    let Some(given) = given.filter(|v| !v.is_empty()) else {
        return Marker::Absent;
    };
    let text = given.to_string_lossy();
    let refuse = |reason| Marker::Refused {
        given: text.clone().into_owned(),
        reason,
    };
    if text == BARE {
        return if bare == BarePolicy::Accept {
            Marker::Held { bare: true }
        } else {
            refuse(Refusal::Bare)
        };
    }
    let Some(id) = text.strip_prefix(MARKER_PREFIX).filter(|id| lease_id(id)) else {
        return refuse(Refusal::Malformed);
    };
    match live(dir, id) {
        Ok(()) => Marker::Held { bare: false },
        Err(reason) => refuse(reason),
    }
}

/// Characters a terminal must not receive raw: controls, and the zero-width
/// and bidirectional marks that reorder a line.
fn needs_escape(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
        )
}

/// `<pid>-<seq>`: two runs of ASCII digits joined by one dash.
fn lease_id(id: &str) -> bool {
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    id.split_once('-')
        .is_some_and(|(pid, seq)| digits(pid) && digits(seq))
}

/// Whether the lease `id` is live in `dir`: a `holders/slot-N.who` record
/// names it and its writer is alive, and one of its slots is locked. The
/// slots are probed under the admission lock, so a probe never races an
/// admission; when that lock is busy, a live record suffices, as
/// [`status_in_on`](crate::status_in_on) allows. The directory is checked,
/// never made. The `admission` file is never removed once created, so when
/// it is absent nothing can be admitting and the slots are probed directly.
/// The record match and the liveness test mirror `read_holders`, the rule
/// `status` reads holders by.
fn live(dir: &Path, id: &str) -> Result<(), Refusal> {
    platform::check_private(dir).map_err(|_| Refusal::Io)?;
    let entries = match fs::read_dir(dir.join("holders")) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Err(Refusal::UnknownLease),
        Err(_) => return Err(Refusal::Io),
    };
    let mut slots = Vec::new();
    let mut dead = false;
    for entry in entries {
        let entry = entry.map_err(|_| Refusal::Io)?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(index) = name
            .strip_prefix("slot-")
            .and_then(|rest| rest.strip_suffix(".who"))
            .and_then(|index| index.parse::<u32>().ok())
        else {
            continue;
        };
        // A record removed or rewritten meanwhile (a release, a rename) is
        // skipped: a lease being renamed keeps its id in the new record.
        let Ok(text) = fs::read_to_string(entry.path()) else {
            continue;
        };
        if raw(&text, "lease").as_deref() != Some(id) {
            continue;
        }
        let holder = Holder::from_json(&text).ok_or(Refusal::Io)?;
        if platform::alive(holder.pid) {
            slots.push(index);
        } else {
            dead = true;
        }
    }
    if slots.is_empty() {
        return Err(if dead {
            Refusal::DeadPid
        } else {
            Refusal::UnknownLease
        });
    }
    let admission = dir.join("admission");
    let _admission = if admission.exists() {
        match platform::try_lock(&admission) {
            Ok(Some(lock)) => Some(lock),
            // An admission in progress: the live record is the answer.
            Ok(None) => return Ok(()),
            Err(_) => return Err(Refusal::Io),
        }
    } else {
        None
    };
    let locked = slots
        .iter()
        .any(|index| platform::is_locked(&dir.join(format!("slot-{index}"))) == Some(true));
    if locked {
        Ok(())
    } else {
        Err(Refusal::SlotFree)
    }
}

/// Once per log, the journal row a marker leaves. A process keeps one in a
/// `static` and passes it to every call, so a refusal seen by many checks or
/// call sites is said and counted once; a test builds its own.
#[derive(Debug, Default)]
pub struct MarkerLog {
    said: AtomicBool,
}

impl MarkerLog {
    /// An empty log, meant for a `static` or a field of the run's settings:
    /// the first refusal it records says its line, later ones say nothing.
    pub const fn new() -> MarkerLog {
        MarkerLog {
            said: AtomicBool::new(false),
        }
    }

    /// Append the row `marker` leaves in `dir`'s `waits.jsonl`, the first
    /// time only; `true` when this call wrote it, so the caller prints its
    /// line then. A refused marker writes `marker_refused` with its `reason`;
    /// any other marker writes nothing. Best-effort, as every journal row.
    ///
    /// The row has no `waited_ms` (wait readers select `has("waited_ms")`,
    /// as with `released` rows), and carries `ppid`, beside the given, cut to
    /// 64 characters and JSON-escaped:
    /// `{"at_ms":…,"tool":…,"pid":…,"ppid":…,"session":…,"marker":…,"reason":…,"outcome":"marker_refused"}`.
    pub fn record(&self, dir: &Path, tool: &str, marker: &Marker) -> bool {
        let Marker::Refused { given, reason } = marker else {
            return false;
        };
        if self.said.swap(true, Ordering::SeqCst) {
            return false;
        }
        append_journal(dir, &row(tool, given, Some(*reason), "marker_refused"));
        true
    }
}

/// One marker row; see [`MarkerLog::record`].
fn row(tool: &str, given: &str, reason: Option<Refusal>, outcome: &str) -> String {
    let session = session_id();
    let session = if session.is_empty() {
        String::new()
    } else {
        format!(",\"session\":\"{}\"", escape(&session))
    };
    let ppid = parent_id().map_or(String::new(), |p| format!(",\"ppid\":{p}"));
    let reason = reason.map_or(String::new(), |r| format!(",\"reason\":\"{}\"", r.as_str()));
    let shown: String = given.chars().take(SHOWN_CHARS).collect();
    format!(
        "{{\"at_ms\":{},\"tool\":\"{}\",\"pid\":{}{ppid}{session},\"marker\":\"{}\"{reason},\"outcome\":\"{outcome}\"}}\n",
        now_ms(),
        escape(tool),
        std::process::id(),
        escape(&shown),
    )
}

#[cfg(unix)]
fn parent_id() -> Option<u32> {
    Some(std::os::unix::process::parent_id())
}

#[cfg(not(unix))]
fn parent_id() -> Option<u32> {
    None
}

/// The two lines `bin` prints for a refused marker, or `None` for any other:
/// what was refused and why, then what to do.
///
/// ```text
/// greengate: GREENHARNESS_HOST_SLOT=1 names no live lease (malformed); queueing
///   unset it; only a slot holder sets it
/// ```
///
/// The control and invisible format characters of the value are escaped
/// first, then it is cut to 64 characters, so a hand-set value can neither
/// drive the terminal nor push the hint out of view.
pub fn refused_line(bin: &str, marker: &Marker) -> Option<String> {
    let Marker::Refused { given, reason } = marker else {
        return None;
    };
    let mut shown = String::new();
    for c in given.chars() {
        if shown.chars().count() >= SHOWN_CHARS {
            break;
        }
        if needs_escape(c) {
            shown.extend(c.escape_default());
        } else {
            shown.push(c);
        }
    }
    let hint = match reason {
        Refusal::Bare => {
            format!("bare held is refused; its holder must be {LEASE_WRITER_SINCE} or later")
        }
        Refusal::DeadPid | Refusal::SlotFree => {
            format!("its holder has ended; see: {bin} slots")
        }
        Refusal::Io => format!("cannot read the slot directory; see: {bin} slots"),
        Refusal::Malformed | Refusal::UnknownLease => {
            "unset it; only a slot holder sets it".to_string()
        }
    };
    Some(format!(
        "{bin}: {}={shown} names no live lease ({}); queueing\n  {hint}",
        crate::HELD_ENV,
        reason.as_str()
    ))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::{Sizing, Waiter};
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "greenharness-marker-{name}-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let _ = fs::remove_dir_all(&dir); // best-effort: a leftover of an earlier run
        fs::create_dir_all(dir.join("holders")).unwrap();
        fs::create_dir_all(dir.join("queue")).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        dir
    }

    fn read(dir: &Path, given: &str) -> Marker {
        held_marker_in(dir, Some(OsStr::new(given)))
    }

    /// A real lease taken in `dir`: its records name its id.
    fn take(dir: &Path) -> crate::Lease {
        let mut waiter =
            Waiter::new_in(dir, Sizing::Fixed(2), 1, Holder::here("test", "marker")).unwrap();
        match waiter.poll().unwrap() {
            crate::Poll::Granted(lease) => lease,
            other => panic!("not granted: {other:?}"),
        }
    }

    fn refused(reason: Refusal) -> impl Fn(&Marker) -> bool {
        move |m| matches!(m, Marker::Refused { reason: r, .. } if *r == reason)
    }

    #[test]
    fn a_live_lease_is_held() {
        let dir = scratch("live");
        let lease = take(&dir);
        assert_eq!(read(&dir, &lease.marker()), Marker::Held { bare: false });
        assert!(lease.marker().starts_with("held:"));
        assert_eq!(lease.marker(), held_value(lease.id()));
        drop(lease);
        let _ = fs::remove_dir_all(&dir); // best-effort: scratch
    }

    #[test]
    fn a_released_lease_is_refused() {
        let dir = scratch("released");
        let lease = take(&dir);
        let marker = lease.marker();
        drop(lease);
        // The release removes the record: no record names it any more.
        assert!(refused(Refusal::UnknownLease)(&read(&dir, &marker)));
        let _ = fs::remove_dir_all(&dir); // best-effort: scratch
    }

    #[test]
    fn a_record_whose_writer_is_dead_is_refused() {
        let dir = scratch("dead");
        // pid 0x7fff_fff0 is never a live process.
        let holder = Holder {
            pid: 0x7fff_fff0,
            ..Holder::here("test", "x")
        };
        fs::write(
            dir.join("holders/slot-0.who"),
            holder.to_json_with(Some("7-0")),
        )
        .unwrap();
        assert!(refused(Refusal::DeadPid)(&read(&dir, "held:7-0")));
        let _ = fs::remove_dir_all(&dir); // best-effort: scratch
    }

    #[test]
    fn a_live_record_whose_slot_is_free_is_refused() {
        let dir = scratch("free");
        // A record this live process wrote, with no flock on slot-1.
        fs::write(
            dir.join("holders/slot-1.who"),
            Holder::here("test", "x").to_json_with(Some("8-0")),
        )
        .unwrap();
        fs::write(dir.join("slot-1"), "").unwrap();
        assert!(refused(Refusal::SlotFree)(&read(&dir, "held:8-0")));
        // An admission in progress: the live record suffices.
        fs::write(dir.join("admission"), "").unwrap();
        let busy = platform::try_lock(&dir.join("admission")).unwrap().unwrap();
        assert_eq!(read(&dir, "held:8-0"), Marker::Held { bare: false });
        drop(busy);
        assert!(refused(Refusal::SlotFree)(&read(&dir, "held:8-0")));
        let _ = fs::remove_dir_all(&dir); // best-effort: scratch
    }

    #[test]
    fn forged_and_odd_values_are_refused() {
        let dir = scratch("forged");
        let _lease = take(&dir);
        for given in [
            "1", "/tmp/x", "held:", "held:x y", "held:1-", "held:-1", "HELD",
        ] {
            assert!(refused(Refusal::Malformed)(&read(&dir, given)), "{given}");
        }
        assert!(refused(Refusal::UnknownLease)(&read(&dir, "held:1-999999")));
        let _ = fs::remove_dir_all(&dir); // best-effort: scratch
    }

    #[test]
    fn unset_and_empty_are_absent() {
        let dir = scratch("absent");
        assert_eq!(held_marker_in(&dir, None), Marker::Absent);
        assert_eq!(read(&dir, ""), Marker::Absent);
        let _ = fs::remove_dir_all(&dir); // best-effort: scratch
    }

    #[test]
    fn bare_held_is_accepted_until_the_rollout_refuses_it() {
        let dir = scratch("bare");
        assert_eq!(read(&dir, "held"), Marker::Held { bare: true });
        let bare = read_marker(&dir, Some(OsStr::new("held")), BarePolicy::Refuse);
        assert!(refused(Refusal::Bare)(&bare));
        let _ = fs::remove_dir_all(&dir); // best-effort: scratch
    }

    #[test]
    fn a_directory_that_is_not_private_is_refused() {
        let dir = scratch("open");
        let lease = take(&dir);
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(refused(Refusal::Io)(&read(&dir, &lease.marker())));
        assert!(refused(Refusal::Io)(&read(
            &dir.join("missing"),
            "held:1-0"
        )));
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        drop(lease);
        let _ = fs::remove_dir_all(&dir); // best-effort: scratch
    }

    #[test]
    fn the_reader_creates_nothing() {
        let dir = scratch("nothing");
        fs::write(
            dir.join("holders/slot-0.who"),
            Holder::here("test", "x").to_json_with(Some("9-0")),
        )
        .unwrap();
        let _ = read(&dir, "held:9-0");
        assert!(!dir.join("admission").exists());
        assert!(!dir.join("slot-0").exists());
        let _ = fs::remove_dir_all(&dir); // best-effort: scratch
    }

    #[test]
    fn a_refusal_is_journalled_once_per_log() {
        let dir = scratch("row");
        let marker = read(&dir, "1");
        let log = MarkerLog::new();
        assert!(log.record(&dir, "greengate", &marker));
        assert!(!log.record(&dir, "greengate", &marker));
        assert!(!MarkerLog::new().record(&dir, "greengate", &Marker::Absent));
        let journal = fs::read_to_string(dir.join("waits.jsonl")).unwrap();
        let rows: Vec<&str> = journal.lines().collect();
        assert_eq!(rows.len(), 1, "{journal}");
        let row = rows[0];
        assert!(row.starts_with("{\"at_ms\":"), "{row}");
        assert!(!row.contains("waited_ms"), "{row}");
        assert!(row.contains("\"ppid\":"), "{row}");
        assert!(row.contains("\"marker\":\"1\""), "{row}");
        assert!(row.contains("\"reason\":\"malformed\""), "{row}");
        assert!(row.ends_with("\"outcome\":\"marker_refused\"}"), "{row}");
        let _ = fs::remove_dir_all(&dir); // best-effort: scratch
    }

    #[test]
    fn a_hostile_value_is_escaped_and_cut() {
        let dir = scratch("hostile");
        let given = format!("x\u{1b}[2J\r\n{}", "é".repeat(80));
        let marker = read(&dir, &given);
        let line = refused_line("greengate", &marker).unwrap();
        let first = line.lines().next().unwrap();
        assert!(
            !first.contains('\u{1b}') && !first.contains('\r'),
            "{first:?}"
        );
        assert!(first.contains("x\\u{1b}[2J\\r\\n"), "{first}");
        assert_eq!(line.lines().count(), 2, "{line}");
        assert!(line.lines().nth(1).unwrap().chars().count() <= 80);
        MarkerLog::new().record(&dir, "greengate", &marker);
        let journal = fs::read_to_string(dir.join("waits.jsonl")).unwrap();
        assert!(!journal.trim_end().contains('\u{1b}'), "{journal:?}");
        assert!(journal.contains("\\u001b"), "{journal}");
        assert_eq!(journal.lines().count(), 1);
        let _ = fs::remove_dir_all(&dir); // best-effort: scratch
    }

    #[test]
    fn every_hint_fits_80_columns() {
        for reason in [
            Refusal::Bare,
            Refusal::Malformed,
            Refusal::UnknownLease,
            Refusal::DeadPid,
            Refusal::SlotFree,
            Refusal::Io,
        ] {
            let marker = Marker::Refused {
                given: "1".into(),
                reason,
            };
            let line = refused_line("greengate", &marker).unwrap();
            let hint = line.lines().nth(1).unwrap();
            assert!(hint.chars().count() <= 80, "{hint}");
            assert!(line.contains(reason.as_str()), "{line}");
        }
        assert_eq!(refused_line("greengate", &Marker::Absent), None);
    }

    #[test]
    fn the_check_is_cheap() {
        let dir = scratch("cheap");
        let lease = take(&dir);
        let marker = lease.marker();
        let mut times: Vec<_> = (0..100)
            .map(|_| {
                let start = std::time::Instant::now();
                assert!(read(&dir, &marker).is_held());
                start.elapsed()
            })
            .collect();
        times.sort();
        let median = times[50];
        eprintln!("held_marker_in median {median:?} over 100 calls");
        assert!(median < std::time::Duration::from_millis(5), "{median:?}");
        drop(lease);
        let _ = fs::remove_dir_all(&dir); // best-effort: scratch
    }
}
