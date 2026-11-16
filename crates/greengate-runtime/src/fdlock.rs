//! The file-descriptor calls a detached land is built from: the per-receipt
//! `flock`, the close-on-exec flag the worker inherits the lock through, and
//! the group kill a stopped worker ends its children with. Externs rather
//! than a dependency, like `posix_signal`: `scripts/check-no-deps.sh` keeps
//! this crate free of them.

use std::fs::File;
use std::os::unix::io::AsRawFd;
use std::time::{Duration, Instant};

extern "C" {
    #[link_name = "flock"]
    fn libc_flock(fd: i32, op: i32) -> i32;
    // Variadic in C: on arm64 macOS a variadic argument is passed on the
    // stack, so the declaration must say so.
    #[link_name = "fcntl"]
    fn libc_fcntl(fd: i32, cmd: i32, ...) -> i32;
    #[link_name = "kill"]
    fn libc_kill(pid: i32, sig: i32) -> i32;
}

const LOCK_EX: i32 = 2;
const LOCK_NB: i32 = 4;
const F_GETFD: i32 = 1;
const F_SETFD: i32 = 2;
const FD_CLOEXEC: i32 = 1;

/// How often a bounded lock wait looks again.
const LOCK_POLL: Duration = Duration::from_millis(25);

/// Take the exclusive lock of `file` without waiting. The lock belongs to the
/// open file description, so it outlives the process that took it for as long
/// as any process holds a copy of the descriptor.
pub(crate) fn try_exclusive(file: &File) -> bool {
    // SAFETY: a valid descriptor owned by `file`; flock touches no memory.
    unsafe { libc_flock(file.as_raw_fd(), LOCK_EX | LOCK_NB) == 0 }
}

/// [`try_exclusive`], retried until `budget` is spent.
pub(crate) fn exclusive_within(file: &File, budget: Duration) -> bool {
    let deadline = Instant::now() + budget;
    loop {
        if try_exclusive(file) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(LOCK_POLL);
    }
}

/// Set `FD_CLOEXEC` on `fd`: an exec'd child does not inherit it. Returns
/// whether the call worked.
pub(crate) fn set_cloexec(fd: i32) -> bool {
    update_fd_flags(fd, |flags| flags | FD_CLOEXEC)
}

/// Clear `FD_CLOEXEC` on `fd`: an exec'd child inherits it. Returns whether
/// the call worked.
pub(crate) fn clear_cloexec(fd: i32) -> bool {
    update_fd_flags(fd, |flags| flags & !FD_CLOEXEC)
}

fn update_fd_flags(fd: i32, change: impl Fn(i32) -> i32) -> bool {
    // SAFETY: fcntl on a descriptor number reads and writes only its flags.
    unsafe {
        let flags = libc_fcntl(fd, F_GETFD);
        if flags < 0 {
            return false;
        }
        libc_fcntl(fd, F_SETFD, change(flags)) == 0
    }
}

/// Send `sig` to every process of group `pgid`.
pub(crate) fn kill_group(pgid: i32, sig: i32) {
    // SAFETY: kill(2) with a negative pid addresses a process group.
    unsafe {
        libc_kill(-pgid, sig);
    }
}
