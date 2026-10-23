//! The bridge between `ra-jit-live` and nsh's engine thread.
//!
//! The daemon answers each edit with a patch: the functions that differ,
//! linked against this executable and written where [`Live::start`]
//! reserved room beside it. `install` polls for patches inside an engine
//! task — a point where no reactive work is half done — and on each one
//! applied, snapshots the [`keep`] signals and bumps `remount`. The
//! `dynamic` region under `remount` then rebuilds: components under it
//! never match an old element (`match_children` in nsh-core's
//! `core/components.rs`), so each is created again and its `view` runs the
//! new code. What `keep` marked holds its value through the swap; nothing
//! else is carried: a patch that changes a type the program can see is
//! answered `Restart` by the daemon, not applied.
//!
//! Applying on the engine thread also matters for what runs while code
//! lands: a function on some thread's stack finishes on its old code and
//! its next call runs the new, so a poll between two engine tasks is the
//! safe point `ra-jit-live` documents.

use std::{
    any::Any,
    cell::RefCell,
    collections::HashMap,
    rc::Rc,
    sync::{Mutex, MutexGuard},
    time::Duration,
};

use nsh_core::ui::prelude::*;

use crate::hot::Game;

/// How often the engine polls for a patch: the apply itself is under a
/// millisecond, so this sets save-to-screen latency more than cost.
const POLL: Duration = Duration::from_millis(25);

// ---------------------------------------------------------------------------
// State anchors — how hot code reaches ONE game and ONE repaint pulse.
//
// A `static` in patchable code is hopeless twice over: a local one has no
// name the splice can match, so each patch emits a private copy of it (the
// "empty board" bug), and a named one is written back to its initializer
// on every apply. A `thread_local` shares on macOS but cannot splice on
// the iOS simulator at all. `ra_jit_live::anchor` is the way out: a
// non-generic function in a dependency — code a patch never re-emits — so
// its map is only ever the executable's one, on every platform.
// ---------------------------------------------------------------------------

/// Called once from `App`'s body — under the root owner, which never
/// remounts. `frame()` anchors the pulse signal while that owner is
/// current, so the signal lives as long as the app; the interval calls
/// `hot::tick` by name and the splice redirects its entry to each new
/// build, so game rules edit live without the clock stopping.
pub(crate) fn start_game() {
    let frame = frame();
    set_interval(Duration::from_millis(50), move || {
        if crate::hot::tick() {
            frame.update(|f| *f += 1);
        }
    });
}

/// The repaint pulse the hot area subscribes to: the same signal the
/// interval bumps, whatever generation of code is asking.
pub(crate) fn frame() -> Signal<u64> {
    let at = ra_jit_live::anchor("stack-native.frame", || {
        Box::leak(Box::new(signal(0u64))) as *const Signal<u64> as usize
    });
    unsafe { *(at as *const Signal<u64>) }
}

/// The one game the whole process has — every patch generation locks the
/// same `Mutex`, so the board survives remounts, not just values.
pub(crate) fn game() -> MutexGuard<'static, Game> {
    let at = ra_jit_live::anchor("stack-native.game", || {
        Box::leak(Box::new(Mutex::new(Game::new()))) as *const Mutex<Game> as usize
    });
    unsafe { &*(at as *const Mutex<Game>) }.lock().unwrap()
}

thread_local! {
    /// The live session (patch room behind the image, the watcher's patch
    /// file). Started once, with the app.
    static LIVE: RefCell<Option<ra_jit_live::Live>> = const { RefCell::new(None) };
    /// What reads each `keep` signal now: refreshed as components mount.
    static READERS: RefCell<HashMap<&'static str, Rc<dyn Fn() -> Box<dyn Any>>>> =
        RefCell::new(HashMap::new());
    /// What the last snapshot left: read once by the next `keep` of the
    /// name, which registers its own reader instead.
    static KEPT: RefCell<HashMap<&'static str, Box<dyn Any>>> = RefCell::new(HashMap::new());
}

/// A signal that survives a hot remount.
///
/// `keep("counter.count", || 0)` reads like `signal(0)` except: before the
/// hot area rebuilds, its value is written into a table under `name`, and
/// the `keep` of the name in the rebuilt tree starts from that value. The
/// name is the identity — keep the same name (and type) across edits, or
/// the old value is dropped and `init` runs.
///
/// The snapshot runs before the rebuild (nsh reconciles new elements
/// before it releases the old ones, so an `on_cleanup` on the outgoing
/// instance would run too late to be read by its replacement).
pub fn keep<T: PartialEq + Clone + 'static>(name: &'static str, init: impl FnOnce() -> T) -> Signal<T> {
    let restored = KEPT.with(|kept| {
        kept.borrow_mut()
            .remove(name)
            .and_then(|value| value.downcast::<T>().ok())
            .map(|value| *value)
    });
    let sig = signal(restored.unwrap_or_else(init));
    READERS.with(|readers| {
        readers.borrow_mut().insert(
            name,
            Rc::new(move || Box::new(sig.get_untracked()) as Box<dyn Any>),
        );
    });
    sig
}

/// Starts `ra_jit_live::Live` and polls it on the engine thread.
///
/// - `Outcome::Applied`: snapshots the `keep` signals, bumps `remount`
///   (which rebuilds the hot area), and reports it on `note`.
/// - `Outcome::Restart` / `Outcome::Declined`: reports on `note`; the app
///   runs on as it was.
///
/// Call it once, under the root, which is never remounted itself: the
/// `Live` it holds and the interval it spawns belong to that owner.
pub fn install(remount: Signal<u64>, note: Signal<String>) {
    LIVE.with(|live| {
        if live.borrow().is_none() {
            *live.borrow_mut() = ra_jit_live::Live::start().ok();
        }
    });
    set_interval(POLL, move || {
        let outcome =
            LIVE.with(|live| live.borrow_mut().as_mut().and_then(ra_jit_live::Live::poll));
        let Some(outcome) = outcome else {
            return;
        };
        match outcome {
            ra_jit_live::Outcome::Applied { .. } => {
                let line = outcome.to_string();
                snapshot();
                remount.update(|v| *v = v.wrapping_add(1));
                note.set(line);
            }
            other => note.set(other.to_string()),
        }
    });
}

/// Reads every `keep` signal into the table its replacement will read.
fn snapshot() {
    READERS.with(|readers| {
        let readers = readers.borrow();
        KEPT.with(|kept| {
            let mut kept = kept.borrow_mut();
            for (name, read) in readers.iter() {
                kept.insert(name, read());
            }
        });
    });
}
