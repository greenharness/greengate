//! `amont slot run` and `amont slots` (ADR-0014): the host slots for any
//! command, and who holds them.
//!
//! `amont slot run [--weight N] [--max-wait S | --now] -- <cmd…>` waits for a
//! slot, runs the command with `GREENHARNESS_HOST_SLOT=held`, passes its exit
//! status through and frees the slot when the command exits. The wrapper
//! alone holds the slot: its descriptors are close-on-exec, so a daemon the
//! command leaves behind never keeps it. A wait ends, with exit 75 and nothing
//! run, when the line makes no progress for 300 s or `--max-wait` runs out
//! (default 90 s, under Claude Code's 120 s Bash timeout) (ADR-0022). `--now`
//! skips the queue up front: the command runs at once, unqueued, journalled
//! `now`.

/// The holder name a `slot run` queues under, as `greengate slots` shows it.
const SLOT_RUN_HOLDER: &str = concat!(crate::bin!(), " slot run");

use crate::names::BIN;
use std::ffi::OsString;
use std::process::Command;
use std::time::{Duration, Instant};

/// Exit status for "the host stayed busy; nothing ran" (EX_TEMPFAIL).
pub const BUSY_EXIT: i32 = 75;

/// Exit status for "the slot queue failed after it was joined; nothing ran"
/// (EX_IOERR): unlike a busy host, retrying the same command fails the same
/// way.
pub const QUEUE_FAILED_EXIT: i32 = 74;

/// The default `--max-wait`: under Claude Code's 120 s Bash timeout, so a
/// wrapped command is never killed while it is still queued.
pub const DEFAULT_MAX_WAIT: Duration = Duration::from_secs(90);

/// `amont slot <args>`.
pub fn slot_command(args: &[OsString]) -> i32 {
    let (own, cmd) = split_at_dashes(args);
    match own.first().and_then(|a| a.to_str()) {
        Some("run") => run(&own[1..], cmd),
        _ => {
            eprintln!(
                "{BIN}: usage: {BIN} slot run [--weight N] [--max-wait S | --now] -- <command…>"
            );
            2
        }
    }
}

/// The arguments before `--`, and the command after it.
pub fn split_at_dashes(args: &[OsString]) -> (&[OsString], &[OsString]) {
    match args.iter().position(|a| a == "--") {
        Some(i) => (&args[..i], &args[i + 1..]),
        None => (args, &[]),
    }
}

/// The flags `slot run` takes with a value. `--on-timeout` is read only to
/// refuse `run` and warn on `exit` (ADR-0022).
const VALUED: &[&str] = &["--weight", "--max-wait", "--on-timeout"];

/// The flags `slot run` takes alone; given, their value is `"true"`.
const SWITCHES: &[&str] = &["--now"];

/// Read `own` (the words before `--`) into flag values, as `--flag value`
/// or `--flag=value`, and switches alone. Anything else is a usage error.
fn parse(own: &[OsString]) -> Result<Vec<(&'static str, String)>, String> {
    let mut out = Vec::new();
    let mut words = own.iter();
    while let Some(word) = words.next() {
        let word = word.to_str().ok_or("an argument is not valid UTF-8")?;
        let (name, inline) = match word.split_once('=') {
            Some((n, v)) => (n, Some(v.to_string())),
            None => (word, None),
        };
        if let Some(switch) = SWITCHES.iter().find(|f| **f == name) {
            if inline.is_some() {
                return Err(format!("{switch} takes no value"));
            }
            out.push((*switch, "true".to_string()));
            continue;
        }
        let Some(flag) = VALUED.iter().find(|f| **f == name) else {
            return Err(format!("unknown argument {word:?}"));
        };
        let value = match inline {
            Some(v) => v,
            None => words
                .next()
                .and_then(|v| v.to_str())
                .ok_or(format!("{flag} needs a value"))?
                .to_string(),
        };
        out.push((*flag, value));
    }
    Ok(out)
}

fn value<'a>(flags: &'a [(&'static str, String)], flag: &str) -> Option<&'a str> {
    flags
        .iter()
        .find(|(f, _)| *f == flag)
        .map(|(_, v)| v.as_str())
}

/// How the command runs: inside a held slot, or with none.
#[derive(Clone, Copy)]
enum Launch {
    /// A slot is held for it (here or by an ancestor): `GREENHARNESS_HOST_SLOT=held`.
    Held,
    /// No slot: off, unsupported, unavailable, or `--now`.
    Unqueued,
}

fn run(own: &[OsString], cmd: &[OsString]) -> i32 {
    let own = match parse(own) {
        Ok(flags) => flags,
        Err(why) => {
            eprintln!("{BIN}: slot run: {why}");
            return 2;
        }
    };
    let own = own.as_slice();
    let Some((program, rest)) = cmd.split_first() else {
        eprintln!("{BIN}: `{BIN} slot run` needs a command after `--`");
        return 2;
    };
    let weight = match value(own, "--weight").map(str::parse::<u32>) {
        None => 1,
        Some(Ok(n)) if n >= 1 => n,
        Some(_) => {
            eprintln!("{BIN}: --weight is a whole number from 1");
            return 2;
        }
    };
    let max_wait = match value(own, "--max-wait").map(str::parse::<u64>) {
        None => DEFAULT_MAX_WAIT,
        Some(Ok(s)) => Duration::from_secs(s),
        Some(Err(_)) => {
            eprintln!("{BIN}: --max-wait is a whole number of seconds");
            return 2;
        }
    };
    match value(own, "--on-timeout") {
        None => {}
        Some("exit") => {
            eprintln!("{BIN}: --on-timeout exit is deprecated and will be removed; drop it")
        }
        Some("run") => {
            eprintln!("{BIN}: --on-timeout was removed (ADR-0022): a wait that ends runs nothing");
            eprintln!("  skip the queue up front: {BIN} slot run --now -- <cmd>");
            return 2;
        }
        Some(other) => {
            eprintln!(
                "{BIN}: --on-timeout was removed (ADR-0022), and {other:?} was never a value"
            );
            return 2;
        }
    }
    let now = value(own, "--now").is_some();
    if now && value(own, "--max-wait").is_some() {
        eprintln!("{BIN}: --now skips the queue, so --max-wait has nothing to bound");
        return 2;
    }
    let label: String = std::iter::once(program)
        .chain(rest.iter().take(2))
        .map(|a| a.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ");

    // Already inside a slot (a wrapped command running this one): no second
    // queue for the slot the parent holds.
    let nested = std::env::var_os(greenharness_slots::HELD_ENV).is_some_and(|v| !v.is_empty());
    let settings = crate::config::Settings::default();
    let (sizing, machine) = crate::host_slots::sizing_and_machine(&settings);
    let (Some(sizing), false, true) = (sizing, nested, greenharness_slots::SUPPORTED) else {
        return spawn(
            program,
            rest,
            if nested {
                Launch::Held
            } else {
                Launch::Unqueued
            },
            None,
        );
    };
    // The width this command may run at, held or not: a `--now` run happens
    // when the host is busy, so it is bounded too.
    let slot_cap = sizing.cap_for(machine.cores);
    let jobs = greenharness_slots::parallelism(weight, machine.cores, slot_cap);

    let dir = greenharness_slots::dir();
    // Every line of this run names the cap and cores it was sized for.
    let record = |waited: u128, outcome: &str, end: Option<greenharness_slots::WaitEnd>| {
        greenharness_slots::record_wait_named(
            &dir,
            SLOT_RUN_HOLDER,
            greenharness_slots::WaitContext {
                cap: Some(slot_cap),
                cores: Some(machine.cores),
                end,
                ..greenharness_slots::WaitContext::default()
            },
            weight,
            Some(jobs),
            waited,
            outcome,
        );
    };
    if now {
        record(0, "now", None);
        return spawn(program, rest, Launch::Unqueued, Some(jobs));
    }
    // Read at the binary's edge: the crate reads no environment for it.
    let setting = std::env::var(greenharness_slots::STALL_ENV).ok();
    let (window, warning) = greenharness_slots::Stall::window_from(setting.as_deref());
    if let Some(warning) = warning {
        eprintln!("{BIN}: warning: {warning}");
    }
    let started = Instant::now();
    let waiter = match greenharness_slots::Waiter::new_in_on(
        &dir,
        sizing,
        weight,
        greenharness_slots::Holder::here(SLOT_RUN_HOLDER, &label),
        machine,
    ) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("{BIN}: host slots unavailable ({e}); running unqueued");
            return spawn(program, rest, Launch::Unqueued, Some(jobs));
        }
    };
    // The caller's deadline is hard (the Bash tool's 120 s); the crate does
    // not apply a stall window as long as it.
    let bounds = greenharness_slots::WaitBounds {
        window: Some(window),
        ceiling: max_wait,
    };
    let mut last_said: Option<Instant> = None;
    let ended = greenharness_slots::wait_in_line(waiter, bounds, |queue| {
        let due = last_said.map_or(true, |t| t.elapsed() >= Duration::from_secs(30));
        if due && (!queue.holders.is_empty() || queue.unknown > 0) {
            eprintln!("{BIN}: {}", crate::host_slots::behind(queue));
            // The pointer once, not on every 30 s repeat.
            if last_said.is_none() {
                eprintln!("  see: {BIN} slots");
            }
            last_said = Some(Instant::now());
        }
    });
    let waited = started.elapsed().as_millis();
    let lease = match ended {
        Ok(lease) => lease,
        Err(end) => {
            record(waited, "timeout", Some(end.end()));
            eprintln!("{BIN}: {}", end_message(&end, max_wait));
            return match end {
                // A broken queue is not a busy host: retrying the same
                // command would fail the same way.
                greenharness_slots::LineEnd::PollFailed { .. } => {
                    eprintln!("  `{label}` did not run (exit {QUEUE_FAILED_EXIT})");
                    QUEUE_FAILED_EXIT
                }
                greenharness_slots::LineEnd::Stalled { .. }
                | greenharness_slots::LineEnd::Ceiling { .. } => {
                    eprintln!("  `{label}` did not run (exit {BUSY_EXIT}); retry later, or pass --now to skip the queue");
                    BUSY_EXIT
                }
            };
        }
    };
    record(waited, "admitted", None);
    let code = spawn(program, rest, Launch::Held, Some(jobs));
    drop(lease);
    code
}

/// Why a `slot run` wait ended, in one line under 80 columns.
fn end_message(end: &greenharness_slots::LineEnd, max_wait: Duration) -> String {
    match end {
        greenharness_slots::LineEnd::Stalled { window, .. } => {
            format!(
                "no progress in the host slot line for {}s",
                window.as_secs()
            )
        }
        greenharness_slots::LineEnd::Ceiling { .. } => {
            format!("no host slot within --max-wait {}s", max_wait.as_secs())
        }
        greenharness_slots::LineEnd::PollFailed { error } => {
            format!("the host slot queue failed while queued: {error}")
        }
    }
}

/// Run the command, inheriting stdio, and pass its exit status through: the
/// code, or 128 + the signal that ended it. `jobs` bounds its width
/// (`greenharness_slots::bound_width`); `None` (slots off, unsupported, or nested
/// under a holder whose own bounds it inherits) sets nothing.
fn spawn(program: &OsString, rest: &[OsString], launch: Launch, jobs: Option<u32>) -> i32 {
    let mut cmd = Command::new(program);
    cmd.args(rest);
    if let Some(jobs) = jobs {
        greenharness_slots::bound_width(&mut cmd, jobs);
    }
    match launch {
        Launch::Held => {
            cmd.env(greenharness_slots::HELD_ENV, "held");
        }
        Launch::Unqueued => {}
    }
    match cmd.status() {
        Ok(status) => exit_code(status),
        Err(e) => {
            eprintln!("{BIN}: cannot run {}: {e}", program.to_string_lossy());
            127
        }
    }
}

fn exit_code(status: std::process::ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(sig) = status.signal() {
            return 128 + sig;
        }
    }
    1
}

/// `amont slots [--json]`.
/// `6 cores, 12 threads`: the threads only where they differ from the cores
/// (SMT), and a short tag when the count did not come from plain detection,
/// so the `slots` header stays inside 80 columns. `doctor` names the key.
pub fn cores_phrase(cores: u32, threads: u32, source: greenharness_slots::host::Source) -> String {
    use greenharness_slots::host::Source;
    let mut phrase = format!("{cores} cores");
    if threads != cores {
        phrase.push_str(&format!(", {threads} threads"));
    }
    let tag = match source {
        Source::Env => Some("env"),
        Source::Global | Source::System => Some("git"),
        Source::LogicalFallback => Some("logical"),
        Source::LimitedByAffinity => Some("affinity"),
        Source::Detected => None,
        // `Source` is non_exhaustive: a new variant needs a tag here.
        _ => None,
    };
    if let Some(tag) = tag {
        phrase.push_str(&format!(" ({tag})"));
    }
    phrase
}

/// Where a host key's value came from, in words a person can act on: the
/// variable or the `git config` scope and key, else the detection outcome.
fn source_label(source: greenharness_slots::host::Source, key: &str, env: &str) -> String {
    use greenharness_slots::host::Source;
    match source {
        Source::Env => env.to_string(),
        Source::Global => format!("git config --global {key}"),
        Source::System => format!("git config --system {key}"),
        other => other.name().to_string(),
    }
}

/// The host sizing lines `doctor` prints, one per key so each fits in 80
/// columns: the core count and the slot count, each with the source that set
/// it. Any warning about either key is printed separately, once, by
/// `host_slots::host_sizing`.
pub fn doctor_line() -> String {
    use greenharness_slots::host::{CORES_ENV, CORES_KEY, SLOTS_ENV, SLOTS_KEY};
    let settings = crate::config::Settings::default();
    let host = crate::host_slots::host_sizing(&settings);
    let slots = match host.slots.value {
        None => "off".to_string(),
        Some(greenharness_slots::Sizing::Auto) => "auto".to_string(),
        Some(greenharness_slots::Sizing::Fixed(n)) => n.to_string(),
    };
    format!(
        "host cores: {}, {} threads ({})\nhost slots: {slots} ({})",
        host.cores.value,
        greenharness_slots::logical_cores(),
        source_label(host.cores.source, CORES_KEY, CORES_ENV),
        source_label(host.slots.source, SLOTS_KEY, SLOTS_ENV),
    )
}

pub fn slots_command(args: &[OsString]) -> i32 {
    if let Some(bad) = args.iter().find(|a| *a != "--json") {
        eprintln!("{BIN}: slots: unknown argument {:?}", bad.to_string_lossy());
        return 2;
    }
    let json = !args.is_empty();
    let settings = crate::config::Settings::default();
    let host = crate::host_slots::host_sizing(&settings);
    let machine = greenharness_slots::Machine {
        cores: host.cores.value.max(1),
    };
    // Additive fields: the core count the slots are sized from, and its source.
    let cores_json = format!(
        "\"cores\":{},\"cores_source\":\"{}\"",
        machine.cores,
        host.cores.source.token()
    );
    let Some(sizing) = host.slots.value else {
        if json {
            println!(
                "{{\"cap\":0,{cores_json},\"held\":0,\"holders\":[],\"unknown\":0,\"waiting\":0}}"
            );
        } else {
            let by = match host.slots.source {
                greenharness_slots::host::Source::Env => {
                    format!("{}=0", greenharness_slots::host::SLOTS_ENV)
                }
                _ => format!("{} 0", greenharness_slots::host::SLOTS_KEY),
            };
            println!("host slots are off ({by})");
        }
        return 0;
    };
    let status = match greenharness_slots::status_in_on(&greenharness_slots::dir(), sizing, machine)
    {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{BIN}: cannot read the host slots: {e}");
            return 1;
        }
    };
    let held = status.holders.len() + status.unknown;
    let mode = match sizing {
        greenharness_slots::Sizing::Auto => format!(
            "auto: load {}, {}",
            greenharness_slots::load1().map_or("?".to_string(), |l| format!("{l:.1}")),
            cores_phrase(
                machine.cores,
                greenharness_slots::logical_cores(),
                host.cores.source
            )
        ),
        greenharness_slots::Sizing::Fixed(n) => format!("greengate.hostSlots {n}"),
    };
    if json {
        let holders: Vec<String> = status
            .holders
            .iter()
            .map(greenharness_slots::Holder::to_json)
            .collect();
        println!(
            "{{\"cap\":{},{cores_json},\"held\":{held},\"holders\":[{}],\"unknown\":{},\"waiting\":{}}}",
            status.cap,
            holders.join(","),
            status.unknown,
            status.waiting
        );
        return 0;
    }
    let now = greenharness_slots::now_ms();
    println!("host slots: {held} of {} held ({mode})", status.cap);
    for h in &status.holders {
        let jobs = h.jobs.map_or("-".to_string(), |j| j.to_string());
        println!(
            "  {}  pid {}  jobs {jobs}  session {}",
            h.describe(now),
            h.pid,
            h.short_session()
        );
    }
    if status.unknown > 0 {
        println!("  {} unknown (old client)", status.unknown);
    }
    println!("waiting: {}", status.waiting);
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(v: &[&str]) -> Vec<OsString> {
        v.iter().map(OsString::from).collect()
    }

    #[test]
    fn the_cores_phrase_names_threads_and_overrides_only_when_they_matter() {
        use greenharness_slots::host::Source;
        assert_eq!(cores_phrase(6, 12, Source::Detected), "6 cores, 12 threads");
        assert_eq!(cores_phrase(10, 10, Source::Detected), "10 cores");
        assert_eq!(
            cores_phrase(4, 12, Source::Env),
            "4 cores, 12 threads (env)"
        );
        assert_eq!(
            cores_phrase(4, 12, Source::Global),
            "4 cores, 12 threads (git)"
        );
        assert_eq!(
            cores_phrase(8, 8, Source::LogicalFallback),
            "8 cores (logical)"
        );
        // A large host with every slot held, a heavy load and the longest tag
        // still fits in 80 columns.
        let header = format!(
            "host slots: 64 of 64 held (auto: load 99.9, {})",
            cores_phrase(128, 256, Source::LimitedByAffinity)
        );
        assert!(header.len() <= 80, "{} columns: {header}", header.len());
    }

    #[test]
    fn flags_take_a_separate_or_an_inline_value_and_nothing_else() {
        let flags = parse(&os(&["--weight", "2", "--on-timeout=run"])).unwrap();
        assert_eq!(value(&flags, "--weight"), Some("2"));
        assert_eq!(value(&flags, "--on-timeout"), Some("run"));
        assert!(parse(&os(&["--wieght", "2"]))
            .unwrap_err()
            .contains("unknown"));
        assert!(parse(&os(&["extra"])).unwrap_err().contains("unknown"));
        assert!(parse(&os(&["--max-wait"]))
            .unwrap_err()
            .contains("needs a value"));
    }

    #[test]
    fn the_command_is_everything_after_the_first_dashes() {
        let args = os(&[
            "run",
            "--weight",
            "2",
            "--",
            "cargo",
            "test",
            "--",
            "--nocapture",
        ]);
        let (own, cmd) = split_at_dashes(&args);
        assert_eq!(own, &os(&["run", "--weight", "2"])[..]);
        assert_eq!(cmd, &os(&["cargo", "test", "--", "--nocapture"])[..]);
    }
}
