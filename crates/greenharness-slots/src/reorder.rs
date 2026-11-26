//! Moving a waiting job ahead (host-slot-admission t05, t06; ADR-0022).
//!
//! A job is moved by renaming its ticket within `queue/` to a name that sorts
//! earlier, under the admission lock. `rename(2)` within one directory is
//! atomic, the waiter's flock follows the inode, and every waiter finds its
//! ticket by inode, so each reads its true place after the move. Who may move
//! what is the person's decision (t06): a session moves only its own jobs, up
//! to the head, at most [`PROMOTE_BUDGET`] times in [`PROMOTE_WINDOW`], never
//! past a job that has waited [`AGE_FLOOR`] or more, and every move is
//! journalled as a `promoted` row with its reason.

use super::{append_journal, escape, live_tickets, platform, raw, ticket_parts, unescape, Holder};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Moves one session may make in [`PROMOTE_WINDOW`] (t06: "starting at 3").
pub const PROMOTE_BUDGET: usize = 3;

/// The rolling window the budget counts over: 24 h, so no time zone is
/// involved.
pub const PROMOTE_WINDOW: Duration = Duration::from_secs(24 * 60 * 60);

/// No move passes a job that has waited this long or longer (t06), so a long
/// waiter is never starved.
pub const AGE_FLOOR: Duration = Duration::from_secs(20 * 60);

/// How long a promotion waits for the admission lock. Each poll holds it
/// for a few milliseconds, so this is only ever reached on a stuck holder.
pub const PROMOTE_LOCK_WAIT: Duration = Duration::from_secs(10);

/// One second in nanoseconds: how far before the head a job moved to the
/// head is named, leaving room for later midpoints behind it.
const HEAD_GAP_NANOS: u128 = 1_000_000_000;

/// One job in the queue, as [`queue_in`] lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct QueuedJob {
    /// `{pid}-{seq}`: the name [`promote_in`] takes.
    pub id: String,
    /// Its place among the live jobs, 0 the head; `None` for a gone one.
    pub position: Option<usize>,
    /// Who waits, from the ticket; `None` for an older client's empty one.
    pub holder: Option<Holder>,
    /// When it joined the queue, ms since the epoch.
    pub joined_ms: u128,
    /// Its waiter died (the ticket's flock is free); the next poll removes it.
    pub gone: bool,
}

/// What a ticket says about its job, parsed.
struct Ticket {
    path: PathBuf,
    nanos: u128,
    id: String,
    holder: Option<Holder>,
    joined_ms: u128,
}

impl Ticket {
    fn read(path: PathBuf) -> Option<Ticket> {
        let name = path.file_name()?.to_str()?;
        let (nanos, id) = ticket_parts(name)?;
        let id = id.to_string();
        let text = fs::read_to_string(&path).unwrap_or_default();
        let holder = Holder::from_json(&text);
        let joined_ms = raw(&text, "joined_ms")
            .and_then(|j| j.parse().ok())
            .filter(|_| holder.is_some())
            .unwrap_or(nanos / 1_000_000);
        Some(Ticket {
            path,
            nanos,
            id,
            holder,
            joined_ms,
        })
    }

    fn waited_ms(&self, now_ms: u128) -> u128 {
        now_ms.saturating_sub(self.joined_ms)
    }
}

/// The queue in [`super::dir`], for `greengate slots`.
pub fn queue() -> io::Result<Vec<QueuedJob>> {
    queue_in(&super::dir())
}

/// The queue in `dir`, in order, read without the admission lock: an
/// observation. Nothing is removed or created. A ticket's liveness is a
/// non-blocking flock on a read-only open, released at once (a pid says
/// nothing across pid namespaces); a dead one is listed `gone`, unnumbered,
/// so the live positions are those a promotion uses. A ticket renamed or
/// removed since the directory was read is skipped.
pub fn queue_in(dir: &Path) -> io::Result<Vec<QueuedJob>> {
    let mut names: Vec<PathBuf> = fs::read_dir(dir.join("queue"))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .and_then(ticket_parts)
                .is_some()
        })
        .collect();
    names.sort();
    let mut jobs = Vec::new();
    let mut position = 0;
    for path in names {
        let Some(locked) = platform::is_locked(&path) else {
            continue; // renamed or removed since the listing
        };
        let Some(ticket) = Ticket::read(path) else {
            continue;
        };
        let gone = !locked;
        jobs.push(QueuedJob {
            id: ticket.id,
            position: (!gone).then_some(position),
            holder: ticket.holder,
            joined_ms: ticket.joined_ms,
            gone,
        });
        if !gone {
            position += 1;
        }
    }
    Ok(jobs)
}

/// A request to move one job ahead.
#[derive(Debug, Clone, Copy)]
pub struct Promote<'a> {
    /// The job, `{pid}-{seq}`.
    pub id: &'a str,
    /// The caller's session ([`super::session_id`], read at the binary's
    /// edge): only a job of this session may move.
    pub session: &'a str,
    /// Why, journalled with the move.
    pub reason: &'a str,
    /// The place to move to, 0 the head.
    pub to: usize,
    /// The tool asking, for the journal row (`greengate`).
    pub tool: &'a str,
    /// Now, ms since the epoch: the age floor and the budget window.
    pub now_ms: u128,
}

/// A move that was made.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Moved {
    pub id: String,
    pub from: usize,
    pub to: usize,
    /// Jobs it passed: `from - to`.
    pub passed: usize,
    /// Moves this session has made in the window, this one included.
    pub used: usize,
    pub budget: usize,
}

/// Why a move was refused: each names the limit it hit ([`Refused::limit`]).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Refused {
    /// The reason was empty.
    NoReason,
    /// The caller has no session id.
    NoSession,
    /// No live ticket has the id.
    NotQueued { id: String },
    /// Two live tickets carry the id (waiters in two pid namespaces).
    Ambiguous { id: String, positions: Vec<usize> },
    /// The ticket is empty: an older client's, which finds itself by path.
    OldClient { id: String },
    /// The job belongs to another session.
    OwnSession { id: String, owner: String },
    /// The job is already at or before the place asked for.
    AlreadyAhead {
        id: String,
        position: usize,
        to: usize,
    },
    /// The session has used its moves in the window.
    Budget {
        used: usize,
        budget: usize,
        /// When the oldest counted move leaves the window, ms from now.
        frees_in_ms: u128,
    },
    /// The move would pass a job that has waited [`AGE_FLOOR`] or more.
    AgeFloor {
        id: String,
        /// The passed job nearest to `id` that has waited too long.
        blocker: String,
        waited_ms: u128,
        /// The furthest place `id` can move to passing no such job, when one
        /// moves it at all.
        furthest: Option<usize>,
    },
    /// No name sorts between the neighbours of the place asked for.
    NoRoom { id: String, to: usize },
}

impl Refused {
    /// The limit's token: `no-reason`, `no-session`, `not-queued`,
    /// `ambiguous`, `old-client`, `own-session`, `already-ahead`, `budget`,
    /// `age-floor`, `no-room`.
    pub fn limit(&self) -> &'static str {
        match self {
            Refused::NoReason => "no-reason",
            Refused::NoSession => "no-session",
            Refused::NotQueued { .. } => "not-queued",
            Refused::Ambiguous { .. } => "ambiguous",
            Refused::OldClient { .. } => "old-client",
            Refused::OwnSession { .. } => "own-session",
            Refused::AlreadyAhead { .. } => "already-ahead",
            Refused::Budget { .. } => "budget",
            Refused::AgeFloor { .. } => "age-floor",
            Refused::NoRoom { .. } => "no-room",
        }
    }

    /// One line saying why, after `refused (<limit>): `.
    pub fn describe(&self) -> String {
        match self {
            Refused::NoReason => "a move needs a reason (--reason <text>)".to_string(),
            Refused::NoSession => format!(
                "no session id: set by the agent ({}), and only a session's own jobs move",
                super::SESSION_ENVS.join(", ")
            ),
            Refused::NotQueued { id } => format!("no waiting job {id}"),
            Refused::Ambiguous { id, positions } => format!(
                "two waiting jobs are named {id} (positions {}); no --to tells them apart",
                positions
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(" and ")
            ),
            Refused::OldClient { id } => {
                format!("{id} was queued by an older client, which cannot be moved")
            }
            Refused::OwnSession { id, owner } => {
                format!("{id} belongs to session {owner}; a session moves only its own jobs")
            }
            Refused::AlreadyAhead { id, position, to } => {
                format!("{id} is at {position}, already at or before {to}")
            }
            Refused::Budget {
                used,
                budget,
                frees_in_ms,
            } => format!(
                "{used} of {budget} moves used in 24 h; the next frees in {}",
                minutes(*frees_in_ms)
            ),
            Refused::AgeFloor {
                id,
                blocker,
                waited_ms,
                ..
            } => format!(
                "{id} would pass {blocker}, which has waited {} (limit {}m)",
                minutes(*waited_ms),
                AGE_FLOOR.as_secs() / 60
            ),
            Refused::NoRoom { id, to } => {
                format!("no name sorts {id} at {to}; try another --to")
            }
        }
    }
}

/// `3h12m`, `23m`, `0m`: a duration in ms, rounded down to the minute.
fn minutes(ms: u128) -> String {
    let m = ms / 60_000;
    if m >= 60 {
        format!("{}h{:02}m", m / 60, m % 60)
    } else {
        format!("{m}m")
    }
}

/// [`promote_in`] in [`super::dir`].
pub fn promote(request: &Promote<'_>) -> io::Result<Result<Moved, Refused>> {
    promote_in(&super::dir(), request)
}

/// Move one job ahead, under t06's rules. `Ok(Err(_))` is a refusal naming
/// its limit, journalled best-effort as `promote_refused`; `Err(_)` is an
/// I/O failure, `WouldBlock` when the admission lock stayed held for
/// [`PROMOTE_LOCK_WAIT`]. `on_lock_wait` runs once when the lock is not free
/// at the first try, so a caller can say it waits.
pub fn promote_in(dir: &Path, request: &Promote<'_>) -> io::Result<Result<Moved, Refused>> {
    promote_in_waiting(dir, request, || {})
}

/// [`promote_in`], calling `on_lock_wait` once when the admission lock is
/// not free at the first try.
pub fn promote_in_waiting(
    dir: &Path,
    request: &Promote<'_>,
    on_lock_wait: impl FnOnce(),
) -> io::Result<Result<Moved, Refused>> {
    let outcome = decide(dir, request, on_lock_wait)?;
    if let Err(refused) = &outcome {
        record_refusal(dir, request, refused);
    }
    Ok(outcome)
}

fn decide(
    dir: &Path,
    request: &Promote<'_>,
    on_lock_wait: impl FnOnce(),
) -> io::Result<Result<Moved, Refused>> {
    if request.reason.trim().is_empty() {
        return Ok(Err(Refused::NoReason));
    }
    if request.session.is_empty() {
        return Ok(Err(Refused::NoSession));
    }
    super::safe_dir(dir).map_err(io::Error::other)?;
    let lock = dir.join("admission");
    let _admission = match platform::try_lock(&lock)? {
        Some(file) => file,
        None => {
            on_lock_wait();
            super::lock_within(&lock, PROMOTE_LOCK_WAIT)?.ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::WouldBlock,
                    format!(
                        "the admission lock stayed held for {}s",
                        PROMOTE_LOCK_WAIT.as_secs()
                    ),
                )
            })?
        }
    };
    let tickets: Vec<Ticket> = live_tickets(dir, None)?
        .into_iter()
        .filter_map(Ticket::read)
        .collect();
    let at: Vec<usize> = (0..tickets.len())
        .filter(|&i| tickets[i].id == request.id)
        .collect();
    let from = match at.as_slice() {
        [] => {
            return Ok(Err(Refused::NotQueued {
                id: request.id.to_string(),
            }))
        }
        [one] => *one,
        many => {
            return Ok(Err(Refused::Ambiguous {
                id: request.id.to_string(),
                positions: many.to_vec(),
            }))
        }
    };
    let target = &tickets[from];
    let id = target.id.clone();
    let Some(holder) = target.holder.as_ref() else {
        return Ok(Err(Refused::OldClient { id }));
    };
    if holder.session != request.session {
        return Ok(Err(Refused::OwnSession {
            id,
            owner: holder.short_session().to_string(),
        }));
    }
    let to = request.to;
    if from <= to {
        return Ok(Err(Refused::AlreadyAhead {
            id,
            position: from,
            to,
        }));
    }
    // The clock is read under the lock, after any wait for it: a job that
    // crossed the age floor while this call waited is not passed. A test's
    // later `now_ms` wins, so the floor and the window can be driven.
    let now = request.now_ms.max(super::now_ms());
    let window = PROMOTE_WINDOW.as_millis();
    let moves = moves_in_window(dir, request.session, now.saturating_sub(window))?;
    if moves.len() >= PROMOTE_BUDGET {
        let oldest = moves.iter().min().copied().unwrap_or(now);
        return Ok(Err(Refused::Budget {
            used: moves.len(),
            budget: PROMOTE_BUDGET,
            frees_in_ms: (oldest + window).saturating_sub(now),
        }));
    }
    let floor = AGE_FLOOR.as_millis();
    if let Some(old) = (to..from)
        .rev()
        .find(|&i| tickets[i].waited_ms(now) >= floor)
    {
        return Ok(Err(Refused::AgeFloor {
            id,
            blocker: tickets[old].id.clone(),
            waited_ms: tickets[old].waited_ms(now),
            furthest: (old + 1 < from).then_some(old + 1),
        }));
    }
    let hi = tickets[to].nanos;
    let nanos = if to == 0 {
        hi.saturating_sub(HEAD_GAP_NANOS)
    } else {
        let lo = tickets[to - 1].nanos;
        if hi.saturating_sub(lo) < 2 {
            return Ok(Err(Refused::NoRoom { id, to }));
        }
        lo + (hi - lo) / 2
    };
    if to == 0 && nanos >= hi {
        return Ok(Err(Refused::NoRoom { id, to }));
    }
    let renamed = dir.join("queue").join(format!("{nanos:024}-{id}"));
    // The row first: a lost rename overcounts one move, a lost row would
    // escape the budget (no signal between the two can leave a move
    // unjournalled).
    append_strict(
        dir,
        &promoted_row(request, now, &id, from, to, &holder.command),
    )?;
    fs::rename(&target.path, &renamed).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("the move of {id} was journalled but its rename failed: {error}"),
        )
    })?;
    Ok(Ok(Moved {
        id,
        from,
        to,
        passed: from - to,
        used: moves.len() + 1,
        budget: PROMOTE_BUDGET,
    }))
}

/// The `at_ms` of this session's `promoted` rows at or after `since_ms`. A
/// missing journal has none; an unreadable one is an error, never zero.
/// Records are split at each `{"at_ms":` start, not only at newlines: every
/// journal row begins so, and a best-effort writer's cut-short line would
/// otherwise swallow the row appended after it.
fn moves_in_window(dir: &Path, session: &str, since_ms: u128) -> io::Result<Vec<u128>> {
    let bytes = match fs::read(dir.join("waits.jsonl")) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let text = String::from_utf8_lossy(&bytes);
    Ok(records(&text)
        .filter(|line| raw(line, "outcome").as_deref() == Some("promoted"))
        .filter(|line| raw(line, "session").map(|s| unescape(&s)).as_deref() == Some(session))
        .filter_map(|line| raw(line, "at_ms")?.parse::<u128>().ok())
        .filter(|at| *at >= since_ms)
        .collect())
}

/// The journal's records: each starts at a `{"at_ms":`.
fn records(text: &str) -> impl Iterator<Item = &str> {
    const START: &str = "{\"at_ms\":";
    let starts: Vec<usize> = text.match_indices(START).map(|(i, _)| i).collect();
    let ends: Vec<usize> = starts.iter().skip(1).copied().chain([text.len()]).collect();
    starts
        .into_iter()
        .zip(ends)
        .map(move |(start, end)| text[start..end].trim_end())
}

fn promoted_row(
    request: &Promote<'_>,
    now: u128,
    id: &str,
    from: usize,
    to: usize,
    command: &str,
) -> String {
    format!(
        "{{\"at_ms\":{},\"tool\":\"{}\",\"pid\":{},\"session\":\"{}\",\"outcome\":\"promoted\",\"id\":\"{}\",\"from\":{from},\"to\":{to},\"passed\":{},\"command\":\"{}\",\"reason\":\"{}\"}}\n",
        now,
        escape(request.tool),
        std::process::id(),
        escape(request.session),
        escape(id),
        from - to,
        escape(command),
        escape(request.reason.trim()),
    )
}

/// A refusal's row, best-effort: it never counts toward the budget, and a
/// lost one costs one sample.
fn record_refusal(dir: &Path, request: &Promote<'_>, refused: &Refused) {
    let line = format!(
        "{{\"at_ms\":{},\"tool\":\"{}\",\"pid\":{},\"session\":\"{}\",\"outcome\":\"promote_refused\",\"id\":\"{}\",\"limit\":\"{}\"}}\n",
        request.now_ms,
        escape(request.tool),
        std::process::id(),
        escape(request.session),
        escape(request.id),
        refused.limit(),
    );
    append_journal(dir, &line);
}

/// Append one line to `waits.jsonl` in a single write, reporting failure:
/// the `promoted` row the budget is counted from must not be lost silently.
fn append_strict(dir: &Path, line: &str) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("waits.jsonl"))?;
    file.write_all(line.as_bytes())
}

#[cfg(all(test, unix))]
mod tests {
    use super::super::{find_ticket, now_ms, ticket_json, Holder, Lease, Poll, Sizing, Waiter};
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    const ME: &str = "session-me-0001";
    const OTHER: &str = "session-other-02";

    fn scratch(tag: &str) -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let d = std::env::temp_dir().join(format!(
            "greengate-reorder-{}-{tag}-{n}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&d); // best-effort: a fresh scratch dir
        super::super::safe_dir(&d).unwrap();
        d
    }

    fn holder(session: &str, command: &str) -> Holder {
        let mut h = Holder::here("t", command);
        h.session = session.to_string();
        h
    }

    fn waiter(dir: &Path, session: &str, command: &str) -> Waiter {
        Waiter::new_in(dir, Sizing::Fixed(1), 1, holder(session, command)).unwrap()
    }

    /// One held slot, so every later waiter waits.
    fn held(dir: &Path) -> Lease {
        let mut w = waiter(dir, OTHER, "holder");
        match w.poll().unwrap() {
            Poll::Granted(lease) => lease,
            Poll::Waiting(q) => panic!("the first waiter must be granted: {q:?}"),
        }
    }

    /// The position a waiter reads once the admission lock is free (a poll
    /// that missed the lock says nothing: `reason: None`).
    fn place(w: &mut Waiter) -> usize {
        loop {
            match w.poll().unwrap() {
                Poll::Waiting(q) if q.reason.is_some() => return q.position,
                Poll::Waiting(_) => std::thread::yield_now(),
                Poll::Granted(_) => panic!("granted while a slot is held"),
            }
        }
    }

    fn ask<'a>(id: &'a str, session: &'a str, to: usize) -> Promote<'a> {
        Promote {
            id,
            session,
            reason: "the person waits on this",
            to,
            tool: "test",
            now_ms: 0,
        }
    }

    fn refused(dir: &Path, request: &Promote<'_>) -> Refused {
        match promote_in(dir, request).unwrap() {
            Err(refused) => refused,
            Ok(moved) => panic!("expected a refusal, moved {moved:?}"),
        }
    }

    fn ids(dir: &Path) -> Vec<String> {
        queue_in(dir)
            .unwrap()
            .into_iter()
            .filter(|j| !j.gone)
            .map(|j| j.id)
            .collect()
    }

    /// A ticket written by hand, held locked by the returned file: an older
    /// client's (empty `content`) or a forged twin.
    fn ticket(dir: &Path, nanos: u128, id: &str, content: &str) -> std::fs::File {
        let path = dir.join("queue").join(format!("{nanos:024}-{id}"));
        let mut file = platform::try_lock(&path).unwrap().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file
    }

    /// Now in full nanoseconds, as a waiter names its ticket: a hand-made
    /// ticket made after a waiter sorts after it.
    fn nanos_now() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }

    fn rows(dir: &Path, outcome: &str) -> usize {
        fs::read_to_string(dir.join("waits.jsonl"))
            .unwrap_or_default()
            .matches(&format!("\"outcome\":\"{outcome}\""))
            .count()
    }

    /// Three waiters of another session poll in a loop while a fourth, of
    /// this session, is moved from 3 to `to`. Answers each waiter's place
    /// afterwards, as `[a, b, c, me]`, and the journal.
    fn move_while_three_poll(tag: &str, to: usize) -> ([usize; 4], String) {
        let d = scratch(tag);
        let _lease = held(&d);
        let mut a = waiter(&d, OTHER, "a");
        let mut b = waiter(&d, OTHER, "b");
        let mut c = waiter(&d, OTHER, "c");
        let mut me = waiter(&d, ME, "me");
        let id = me.id().to_string();
        assert_eq!(place(&mut me), 3);
        let stop = AtomicBool::new(false);
        let errors = AtomicUsize::new(0);
        std::thread::scope(|s| {
            for w in [&mut a, &mut b, &mut c] {
                let (stop, errors) = (&stop, &errors);
                s.spawn(move || {
                    while !stop.load(Ordering::SeqCst) {
                        if w.poll().is_err() {
                            errors.fetch_add(1, Ordering::SeqCst);
                        }
                        std::thread::sleep(Duration::from_millis(1));
                    }
                });
            }
            let moved = promote_in(&d, &ask(&id, ME, to)).unwrap().unwrap();
            assert_eq!((moved.from, moved.to, moved.passed), (3, to, 3 - to));
            assert_eq!((moved.used, moved.budget), (1, PROMOTE_BUDGET));
            stop.store(true, Ordering::SeqCst);
        });
        assert_eq!(
            errors.load(Ordering::SeqCst),
            0,
            "a poll erred during the move"
        );
        let places = [place(&mut a), place(&mut b), place(&mut c), place(&mut me)];
        (places, fs::read_to_string(d.join("waits.jsonl")).unwrap())
    }

    /// t13's test, in its own words: a waiter moves from position 3 to 0
    /// while three waiters poll, and every waiter reads its true position.
    #[test]
    fn three_waiters_polling_read_their_true_place_after_a_move_from_three_to_zero() {
        let (places, journal) = move_while_three_poll("move0", 0);
        assert_eq!(places, [1, 2, 3, 0]);
        assert_eq!(journal.matches("\"outcome\":\"promoted\"").count(), 1);
    }

    /// The same move to 1: the old `unwrap_or(0)` read a lost ticket as the
    /// head, so only a move short of the head proves the find-by-inode fix.
    #[test]
    fn three_waiters_polling_read_their_true_place_after_a_move_from_three_to_one() {
        let (places, row) = move_while_three_poll("move1", 1);
        assert_eq!(places, [0, 2, 3, 1]);
        for field in [
            "\"from\":3",
            "\"to\":1",
            "\"passed\":2",
            "\"command\":\"me\"",
            "\"reason\":\"the person waits on this\"",
        ] {
            assert!(row.contains(field), "{field} missing: {row}");
        }
        assert!(!row.contains("waited_ms"), "{row}");
    }

    /// A waiter whose ticket vanished errs; it never reads itself the head.
    #[test]
    fn a_waiter_whose_ticket_is_gone_errs_not_found() {
        let d = scratch("gone");
        let _lease = held(&d);
        let mut w = waiter(&d, ME, "w");
        let path = find_ticket(&d, w.key).unwrap();
        fs::remove_file(path).unwrap();
        let error = w.poll().unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound, "{error}");
    }

    /// Two tickets with one id (two pid namespaces): each waiter finds its
    /// own by inode, and a move of that id is refused.
    #[test]
    fn a_shared_id_confuses_no_waiter_and_is_refused_as_ambiguous() {
        let d = scratch("twin");
        let _lease = held(&d);
        let mut me = waiter(&d, ME, "me");
        let id = me.id().to_string();
        let mut other = waiter(&d, OTHER, "later");
        let twin = ticket(
            &d,
            nanos_now() + 1_000_000_000,
            &id,
            &ticket_json(&holder(ME, "twin"), now_ms()),
        );
        assert_eq!([place(&mut me), place(&mut other)], [0, 1]);
        let r = refused(&d, &ask(&id, ME, 0));
        assert_eq!(r.limit(), "ambiguous");
        assert_eq!(
            r,
            Refused::Ambiguous {
                id: id.clone(),
                positions: vec![0, 2]
            }
        );
        drop(me);
        assert!(find_ticket(&d, platform::file_key(&twin.metadata().unwrap())).is_some());
    }

    /// Every limit is refused by name, and the queue stays as it was.
    #[test]
    fn each_limit_is_refused_by_name_and_moves_nothing() {
        let d = scratch("limits");
        let _lease = held(&d);
        let _a = waiter(&d, OTHER, "a");
        let theirs = waiter(&d, OTHER, "theirs");
        let mine = waiter(&d, ME, "mine");
        let old = ticket(&d, nanos_now() + 1_000_000_000, "999999-7", "");
        let before = ids(&d);
        let cases = [
            (
                Promote {
                    reason: "  ",
                    ..ask(mine.id(), ME, 0)
                },
                "no-reason",
            ),
            (ask(mine.id(), "", 0), "no-session"),
            (ask("424242-0", ME, 0), "not-queued"),
            (ask("999999-7", ME, 0), "old-client"),
            (ask(theirs.id(), ME, 0), "own-session"),
            (ask(mine.id(), ME, 2), "already-ahead"),
        ];
        for (request, limit) in cases {
            assert_eq!(refused(&d, &request).limit(), limit, "{request:?}");
        }
        assert_eq!(ids(&d), before);
        assert_eq!(rows(&d, "promote_refused"), 6);
        assert_eq!(rows(&d, "promoted"), 0);
        drop(old);
    }

    /// Three moves in 24 h use the budget; an older move and refusals do
    /// not count, nor does a cut-short line hide a move.
    #[test]
    fn the_budget_counts_this_sessions_moves_in_the_last_24_hours() {
        let d = scratch("budget");
        let _lease = held(&d);
        let _a = waiter(&d, OTHER, "a");
        let mine = waiter(&d, ME, "mine");
        let now = now_ms();
        let day = PROMOTE_WINDOW.as_millis();
        let row = |at: u128, session: &str, outcome: &str| {
            format!("{{\"at_ms\":{at},\"tool\":\"t\",\"pid\":1,\"session\":\"{session}\",\"outcome\":\"{outcome}\"}}\n")
        };
        let mut journal = String::new();
        journal.push_str(&row(now - day - 60_000, ME, "promoted")); // left the window
        journal.push_str(&row(now - 1_000, OTHER, "promoted")); // another session
        for _ in 0..3 {
            journal.push_str(&row(now - 1_000, ME, "promote_refused"));
        }
        journal.push_str(&row(now - 3_600_000, ME, "promoted"));
        journal.push_str("{\"at_ms\":17,\"tool\":\"cut"); // a write cut short
        journal.push_str(&row(now - 2_000, ME, "promoted"));
        fs::write(d.join("waits.jsonl"), &journal).unwrap();
        // Two counted: one move left.
        let moved = promote_in(&d, &ask(mine.id(), ME, 0)).unwrap().unwrap();
        assert_eq!((moved.used, moved.budget), (3, 3));
        let _b = waiter(&d, OTHER, "b");
        let r = refused(&d, &ask(mine.id(), ME, 0));
        assert_eq!(r.limit(), "already-ahead");
        let late = waiter(&d, ME, "late");
        let r = refused(&d, &ask(late.id(), ME, 0));
        match r {
            Refused::Budget {
                used,
                budget,
                frees_in_ms,
            } => {
                assert_eq!((used, budget), (3, 3));
                // The oldest counted move (1 h ago) frees in about 23 h.
                assert!(
                    frees_in_ms > day - 3_700_000 && frees_in_ms <= day - 3_600_000,
                    "{frees_in_ms}"
                );
            }
            other => panic!("expected budget, got {other:?}"),
        }
    }

    /// No move passes a job that has waited 20 min; the refusal names the
    /// furthest place that passes none.
    #[test]
    fn the_age_floor_refuses_and_names_the_furthest_place() {
        let d = scratch("floor");
        let _lease = held(&d);
        // An older client's empty ticket, aged by its name: 30 min ago.
        let _old = ticket(&d, nanos_now() - 30 * 60 * 1_000_000_000, "999999-1", "");
        let _a = waiter(&d, OTHER, "a");
        let _b = waiter(&d, OTHER, "b");
        let mine = waiter(&d, ME, "mine");
        let before = ids(&d);
        match refused(&d, &ask(mine.id(), ME, 0)) {
            Refused::AgeFloor {
                blocker,
                waited_ms,
                furthest,
                ..
            } => {
                assert_eq!(blocker, "999999-1");
                assert!(waited_ms >= AGE_FLOOR.as_millis(), "{waited_ms}");
                assert_eq!(furthest, Some(1));
            }
            other => panic!("expected age-floor, got {other:?}"),
        }
        assert_eq!(ids(&d), before);
        // The furthest place is allowed.
        let moved = promote_in(&d, &ask(mine.id(), ME, 1)).unwrap().unwrap();
        assert_eq!((moved.from, moved.to), (3, 1));
        // Every job past the floor: no place is offered.
        let r = refused(
            &d,
            &Promote {
                now_ms: now_ms() + 25 * 60 * 1000,
                ..ask(_b.id(), OTHER, 0)
            },
        );
        assert!(
            matches!(r, Refused::AgeFloor { furthest: None, .. }),
            "{r:?}"
        );
    }

    /// A move to the head leaves room behind it: a later move to 1 lands
    /// between the moved job and the old head.
    #[test]
    fn a_move_to_the_head_leaves_room_for_a_move_to_one() {
        let d = scratch("room");
        let _lease = held(&d);
        let mut head = waiter(&d, OTHER, "head");
        let _x = waiter(&d, OTHER, "x");
        let mut first = waiter(&d, ME, "first");
        let mut second = waiter(&d, ME, "second");
        promote_in(&d, &ask(first.id(), ME, 0)).unwrap().unwrap();
        promote_in(&d, &ask(second.id(), ME, 1)).unwrap().unwrap();
        assert_eq!(
            [place(&mut first), place(&mut second), place(&mut head)],
            [0, 1, 2]
        );
    }

    /// An empty (crate 5.1) ticket passed by a move is one place further
    /// back; a promoted waiter that leaves takes its renamed ticket along.
    #[test]
    fn an_old_ticket_is_passed_and_a_moved_waiter_leaves_cleanly() {
        let d = scratch("passed");
        let _lease = held(&d);
        let _old = ticket(&d, nanos_now(), "999999-2", "");
        let mine = waiter(&d, ME, "mine");
        promote_in(&d, &ask(mine.id(), ME, 0)).unwrap().unwrap();
        let jobs = queue_in(&d).unwrap();
        let old = jobs.iter().find(|j| j.id == "999999-2").unwrap();
        assert_eq!(old.position, Some(1));
        assert!(old.holder.is_none());
        drop(mine);
        assert_eq!(ids(&d), vec!["999999-2".to_string()]);
    }

    /// The `promoted` row is written before the rename: a journal that
    /// cannot be written moves nothing.
    #[test]
    fn an_unwritable_journal_moves_nothing() {
        let d = scratch("journal");
        let _lease = held(&d);
        let _a = waiter(&d, OTHER, "a");
        let mut mine = waiter(&d, ME, "mine");
        fs::create_dir(d.join("waits.jsonl")).unwrap();
        assert!(promote_in(&d, &ask(mine.id(), ME, 0)).is_err());
        assert_eq!(place(&mut mine), 1);
    }

    /// With no journal yet, the first move is made and starts it.
    #[test]
    fn the_first_move_starts_the_journal() {
        let d = scratch("first");
        let _lease = held(&d);
        let _ = fs::remove_file(d.join("waits.jsonl")); // the holder's line, if any
        let _a = waiter(&d, OTHER, "a");
        let mine = waiter(&d, ME, "mine");
        assert!(!d.join("waits.jsonl").exists());
        promote_in(&d, &ask(mine.id(), ME, 0)).unwrap().unwrap();
        assert_eq!(rows(&d, "promoted"), 1);
    }

    /// The listing: who waits, in order; a dead ticket is `gone` and
    /// unnumbered; nothing is created or removed.
    #[test]
    fn the_queue_lists_who_waits_and_marks_dead_tickets_gone() {
        let d = scratch("list");
        let _lease = held(&d);
        let a = waiter(&d, OTHER, "cargo test");
        let dead = d
            .join("queue")
            .join(format!("{:024}-999999-3", nanos_now()));
        fs::write(&dead, "").unwrap();
        let b = waiter(&d, ME, "clippy");
        let jobs = queue_in(&d).unwrap();
        let view: Vec<(Option<usize>, &str, bool)> = jobs
            .iter()
            .map(|j| (j.position, j.id.as_str(), j.gone))
            .collect();
        assert_eq!(
            view,
            vec![
                (Some(0), a.id(), false),
                (None, "999999-3", true),
                (Some(1), b.id(), false),
            ]
        );
        let h = jobs[2].holder.as_ref().unwrap();
        assert_eq!((h.session.as_str(), h.command.as_str()), (ME, "clippy"));
        assert!(dead.exists(), "the listing removes nothing");
    }

    /// The budget scan over a month of journal (about 14k lines) stays
    /// short: it runs under the admission lock.
    #[test]
    fn the_budget_scan_over_a_month_of_journal_is_short() {
        let d = scratch("month");
        let line = "{\"at_ms\":1,\"tool\":\"greengate\",\"pid\":1,\"session\":\"x\",\"weight\":1,\"cores\":6,\"holders\":1,\"holder_jobs\":2,\"waited_ms\":100,\"load1\":3.00,\"outcome\":\"admitted\"}\n";
        fs::write(d.join("waits.jsonl"), line.repeat(14_000)).unwrap();
        let started = std::time::Instant::now();
        let moves = moves_in_window(&d, ME, 0).unwrap();
        let took = started.elapsed();
        eprintln!("budget scan over 14k lines: {took:?}");
        assert!(moves.is_empty());
        assert!(took < Duration::from_secs(1), "{took:?}");
    }
}
