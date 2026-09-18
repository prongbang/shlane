//! Ctrl-C handling.
//!
//! Pressing Ctrl-C should stop the command that is running, run the config's
//! `error` hooks, and exit 130 — not leave a build running in the background
//! (`docs/plan/04-cli-ux.md`).
//!
//! The handler itself does as little as possible: set a flag and signal the
//! child's process group. Everything else happens on the main thread, which
//! notices the flag when the child exits.
//!
//! Steps run in their own process group (see `shell`), so the terminal does not
//! deliver Ctrl-C to them; forwarding it here is what stops the running build.

use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

static INTERRUPTED: AtomicBool = AtomicBool::new(false);

/// The running child, or 0.
///
/// A terminal delivers Ctrl-C to the whole foreground group, so the child
/// usually dies on its own -- but a signal sent to shlane alone (a CI runner
/// shutting a job down, `kill -INT <pid>`) would leave it running, which is
/// exactly the build that must not survive.
static CURRENT_CHILD: AtomicI32 = AtomicI32::new(0);

/// Whether that child is the leader of its own process group.
static CHILD_IS_GROUP: AtomicBool = AtomicBool::new(false);

pub fn install() {
    #[cfg(unix)]
    {
        // Safety: installing a handler before any threads are spawned. The
        // handler only touches atomics and calls killpg, which is
        // async-signal-safe.
        let handler = handle as extern "C" fn(libc::c_int) as libc::sighandler_t;
        unsafe {
            libc::signal(libc::SIGINT, handler);
            libc::signal(libc::SIGTERM, handler);
        }
    }
}

#[cfg(unix)]
extern "C" fn handle(_signal: libc::c_int) {
    INTERRUPTED.store(true, Ordering::SeqCst);
    let child = CURRENT_CHILD.load(Ordering::SeqCst);
    if child > 0 {
        // Safety: these only signal; a process that has already exited returns
        // ESRCH, which is why the result is ignored. Both are
        // async-signal-safe.
        unsafe {
            if CHILD_IS_GROUP.load(Ordering::SeqCst) {
                libc::killpg(child, libc::SIGTERM);
            } else {
                libc::kill(child, libc::SIGTERM);
            }
        }
    }
}

pub fn interrupted() -> bool {
    INTERRUPTED.load(Ordering::SeqCst)
}

pub fn register_child(pid: i32, own_group: bool) {
    CHILD_IS_GROUP.store(own_group, Ordering::SeqCst);
    CURRENT_CHILD.store(pid, Ordering::SeqCst);
}

pub fn clear_child() {
    CURRENT_CHILD.store(0, Ordering::SeqCst);
    CHILD_IS_GROUP.store(false, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs everywhere: which child is being tracked is the same bookkeeping on
    /// every platform, even where there is no signal to forward it to. Without
    /// a test that compiles on Windows this module was empty there, and the
    /// `use super::*` above became an unused import -- which `-D warnings`
    /// turns into a build failure.
    #[test]
    fn a_registered_child_is_forgotten_once_it_is_cleared() {
        register_child(4242, true);
        assert_eq!(CURRENT_CHILD.load(Ordering::SeqCst), 4242);
        assert!(CHILD_IS_GROUP.load(Ordering::SeqCst));

        clear_child();
        assert_eq!(CURRENT_CHILD.load(Ordering::SeqCst), 0);
        assert!(!CHILD_IS_GROUP.load(Ordering::SeqCst));
    }

    #[cfg(unix)]
    #[test]
    fn the_handler_records_an_interrupt() {
        install();
        // Safety: raising a signal this process handles.
        unsafe {
            libc::raise(libc::SIGINT);
        }
        assert!(interrupted(), "the SIGINT handler did not run");
    }
}
