//! The bounds of a script runtime (spec 041, R3.3) and the way they are held: QuickJS's own memory
//! limit, an interrupt handler reading the engine's monotonic clock, a stack limit, and a rule for
//! the garbage collector. The runtimes themselves come from the `ScriptRuntimes` adapter: the host
//! uses the system's allocator (`SystemRuntimes`), the firmware its PSRAM allocator.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use rquickjs::Runtime;

use crate::recipes::ScriptClock;

/// Source text of one script recipe, at most. Worst case: 2 scripts of 32 KB kept as text (the
/// source stays, for the runtime to be made again).
pub const MAX_SCRIPT_BYTES: usize = 32 * 1024;
/// The memory limit per runtime on the box: the allocator's (`PsramAllocator` counts the usable
/// size of each block and refuses past it; QuickJS itself is given no limit there). On the Mac
/// QuickJS's own limit holds, `SYSTEM_MEMORY_LIMIT` below.
/// Worst case: 4 runtimes (`MAX_SCRIPT_RECIPES`) of 224 KB, 896 KB, on their own line of the memory
/// budget (spec 053; a script of 32 KB with 4 instances holds 180,668 bytes in QEMU, and its load
/// ran out of memory at 208 KB: the limit stays 224 KB, D85). It holds `MAX_INSTANCES_PER_SCRIPT` instances (measured
/// in QEMU: about 8-10 KB each, on a runtime of about 107 KB).
pub const SCRIPT_MEMORY_LIMIT: usize = 224 * 1024;
/// The same limit on the Mac (`SystemRuntimes`), twice the box's, 448 KB: QuickJS's objects hold
/// 8-byte pointers there and `malloc_size` rounds each block up, so the same runtime counts about
/// 1.6 times the 32-bit figure (measured: `motion-light` with 8 instances holds about
/// 206 KB on the Mac, over 2 times under this limit).
pub const SYSTEM_MEMORY_LIMIT: usize = 2 * SCRIPT_MEMORY_LIMIT;
/// One call into a script takes at most this long (the interrupt handler stops it). Measured on
/// the box: a call of `motion-light` takes about 24 ms on average and up to 83 ms at 160 MHz
/// (2026-11-26); 200 ms leaves room for the slowest without letting a loop hold the engine long.
pub const SCRIPT_CALL_LIMIT_MS: u64 = 200;
/// Loading a script (compile, top level, the `recipe()` call, its info) takes at most this long:
/// it is done once, and compiling is slow on the box.
pub const SCRIPT_LOAD_LIMIT_MS: u64 = 1000;
/// QuickJS's stack limit: a deep recursion stops at this many bytes of stack, on whichever thread
/// the call is made.
pub const SCRIPT_STACK_BYTES: usize = 16 * 1024;
/// Timers an instance holds, at most. Worst case: 16 entries of about 100 bytes per instance.
pub const MAX_TIMERS: usize = 16;
/// Lines one call logs, at most; one more says `Log lines dropped`.
pub const MAX_LOG_LINES_PER_CALL: usize = 20;
/// An alias a script listens to is at most this many bytes (Sowel's aliases are a few words). The
/// shim refuses a longer one; the engine checks the end of each call again.
pub const MAX_ALIAS_BYTES: usize = 64;
/// A log line the shim hands the engine is at most this many bytes (400 characters of up to 4
/// bytes): the engine checks it again at the end of each call.
pub const MAX_END_LINE_BYTES: usize = 1600;
/// The garbage collector runs by itself above this part of the memory limit (three quarters)...
const GC_THRESHOLD_NUM: usize = 3;
const GC_THRESHOLD_DEN: usize = 4;
/// ... and after a call that leaves more than 85 % of it held, or more than `GC_GROWTH_BYTES` more
/// than the last collection left (an instance that leaks is collected well before the limit; a
/// runtime that merely holds its instances is not collected after every call). A runtime that,
/// after that, still holds nine tenths of its limit has failed in all but name.
const GC_AFTER_CALL_PERCENT: usize = 85;
pub const GC_GROWTH_BYTES: usize = 32 * 1024;
const HELD_AFTER_GC_NUM: usize = 9;
const HELD_AFTER_GC_DEN: usize = 10;

/// Set by a runtime's allocator when it refuses an allocation (one flag per runtime).
pub type RefusedFlag = Arc<AtomicBool>;
/// The bytes a runtime's allocator holds, counted as it goes (one per runtime).
pub type HeldCounter = Arc<AtomicUsize>;

/// What `ScriptRuntimes::new_runtime` makes.
pub struct NewRuntime {
    pub runtime: Runtime,
    /// The runtime's own refusal flag, when its allocator can tell; the system's cannot.
    pub refused: Option<RefusedFlag>,
    /// What the runtime's allocator holds, when it counts it: read after each call in place of
    /// QuickJS's own accounting, which walks the whole heap (slow on the box, where the heap is in
    /// PSRAM). The system's allocator does not count: `None`.
    pub held: Option<HeldCounter>,
}

/// Where the engine gets its runtimes. The core never touches the allocator: the host's default
/// is `SystemRuntimes`, the firmware gives a runtime over its own allocator, which also tells
/// when it refused an allocation.
pub trait ScriptRuntimes: Send + Sync {
    /// A new runtime, with no limit set (the core sets them), and its own refusal flag when its
    /// allocator holds it to the limit itself: the flag is set when the allocator refused an
    /// allocation, and a script that caught the error still fails the recipe. The core clears it
    /// before each call and each load. The system's allocator cannot tell (QuickJS's own limit
    /// reports through the error only): no flag, and the core then also fails a call that ends
    /// holding almost its whole limit.
    fn new_runtime(&self) -> Result<NewRuntime, String>;

    /// The memory limit of a runtime: what the 90 % rule and the collector's thresholds are
    /// counted against.
    fn memory_limit(&self) -> usize {
        SCRIPT_MEMORY_LIMIT
    }

    /// QuickJS's own memory limit (0: none, the allocator holds the limit itself and QuickJS's
    /// accounting, which is not the allocator's, never refuses first).
    fn quickjs_limit(&self) -> usize {
        self.memory_limit()
    }
}

/// QuickJS over the system's allocator, which counts with `malloc_size`: the host's runtimes.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemRuntimes;

impl ScriptRuntimes for SystemRuntimes {
    fn new_runtime(&self) -> Result<NewRuntime, String> {
        Ok(NewRuntime {
            runtime: Runtime::new().map_err(|e| e.to_string())?,
            refused: None,
            held: None,
        })
    }

    fn memory_limit(&self) -> usize {
        SYSTEM_MEMORY_LIMIT
    }
}

/// Why a call was stopped from outside the script.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// The call ran past `SCRIPT_CALL_LIMIT_MS`.
    Time,
    /// The script called the engine with a handle that is not one of its instances'.
    ForgedHandle,
}

impl std::fmt::Display for Reason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Reason::Time => write!(f, "took more than {SCRIPT_CALL_LIMIT_MS} ms"),
            Reason::ForgedHandle => {
                f.write_str("called the engine with a handle that is not its own")
            }
        }
    }
}

/// What the interrupt handler reads, and what the host functions can set: the time the current
/// call must end by, and a kill switch with its reason.
#[derive(Debug, Default)]
pub struct Gate {
    /// A mutex, not an atomic: the Xtensa target has no 64-bit atomics.
    deadline: Mutex<u64>,
    killed: AtomicBool,
    reason: Mutex<Option<Reason>>,
}

impl Gate {
    /// A new call starts: it must end by `deadline` (milliseconds of the engine's clock).
    pub fn arm(&self, deadline: u64) {
        self.killed.store(false, Ordering::SeqCst);
        *self.reason.lock().unwrap_or_else(PoisonError::into_inner) = None;
        *self.deadline.lock().unwrap_or_else(PoisonError::into_inner) = deadline;
    }

    /// Stops the call at the interrupt handler's next look, for this reason (the first one is
    /// kept).
    pub fn kill(&self, reason: Reason) {
        let mut slot = self.reason.lock().unwrap_or_else(PoisonError::into_inner);
        if slot.is_none() {
            *slot = Some(reason);
        }
        self.killed.store(true, Ordering::SeqCst);
    }

    /// True once the call was stopped from outside.
    pub fn is_killed(&self) -> bool {
        self.killed.load(Ordering::SeqCst)
    }

    /// Why the call was stopped, if it was (and forgets it).
    pub fn take(&self) -> Option<Reason> {
        self.killed.store(false, Ordering::SeqCst);
        self.reason
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }
}

/// Sets the limits of a new runtime: `quickjs_limit` is QuickJS's own (0: none), `limit` the one
/// the collector's thresholds are counted against.
pub fn configure(
    runtime: &Runtime,
    gate: &Arc<Gate>,
    clock: &ScriptClock,
    limit: usize,
    quickjs_limit: usize,
) {
    runtime.set_memory_limit(quickjs_limit);
    runtime.set_max_stack_size(SCRIPT_STACK_BYTES);
    runtime.set_gc_threshold(limit / GC_THRESHOLD_DEN * GC_THRESHOLD_NUM);
    let (gate, clock) = (Arc::clone(gate), Arc::clone(clock));
    runtime.set_interrupt_handler(Some(Box::new(move || {
        if gate.killed.load(Ordering::SeqCst) {
            return true;
        }
        let deadline = *gate.deadline.lock().unwrap_or_else(PoisonError::into_inner);
        if clock() >= deadline {
            gate.kill(Reason::Time);
            return true;
        }
        false
    })));
}

/// Before a call: the clock starts, the refusal flag is cleared (a refusal of an earlier load or
/// stop is not this call's), and the collector's threshold is back at three quarters of the limit
/// (QuickJS moves it after each collection).
pub fn before_call(
    runtime: &Runtime,
    gate: &Gate,
    clock: &ScriptClock,
    limit: usize,
    refused: &Option<RefusedFlag>,
) {
    gate.arm(clock().saturating_add(SCRIPT_CALL_LIMIT_MS));
    runtime.set_gc_threshold(limit / GC_THRESHOLD_DEN * GC_THRESHOLD_NUM);
    clear(refused);
}

/// Before loading a script: the clock starts with `SCRIPT_LOAD_LIMIT_MS`, not the call's.
pub fn before_load(
    runtime: &Runtime,
    gate: &Gate,
    clock: &ScriptClock,
    limit: usize,
    refused: &Option<RefusedFlag>,
) {
    gate.arm(clock().saturating_add(SCRIPT_LOAD_LIMIT_MS));
    runtime.set_gc_threshold(limit / GC_THRESHOLD_DEN * GC_THRESHOLD_NUM);
    clear(refused);
}

fn clear(refused: &Option<RefusedFlag>) {
    if let Some(flag) = refused {
        flag.store(false, Ordering::SeqCst);
    }
}

/// True when the runtime's allocator refused an allocation since `before_call`/`before_load`.
pub fn was_refused(refused: &Option<RefusedFlag>) -> bool {
    refused
        .as_ref()
        .is_some_and(|flag| flag.swap(false, Ordering::SeqCst))
}

/// The bytes the runtime holds now: the allocator's count when it keeps one, else QuickJS's own
/// accounting (which walks the whole heap).
pub fn held(runtime: &Runtime, counter: &Option<HeldCounter>) -> usize {
    match counter {
        Some(counter) => counter.load(Ordering::Relaxed),
        None => usize::try_from(runtime.memory_usage().malloc_size).unwrap_or(0),
    }
}

/// What a call left the runtime holding.
#[derive(Debug, Clone, Copy)]
pub struct AfterCall {
    /// The runtime still holds almost all its limit after the collection (the call as good as ran
    /// out of memory, caught or not).
    pub exhausted: bool,
    /// What it held when the call ended, before any collection (the peak the counters keep).
    pub at_end: usize,
    /// What it holds now (after the collection, when there was one).
    pub now: usize,
}

/// After a call: `held` is read once (without the allocator's count, it walks the whole heap); the
/// collector runs when the runtime holds more than 85 % of its limit or `GC_GROWTH_BYTES` more than
/// the last collection left (`after_gc`, kept by the caller).
pub fn after_call(
    runtime: &Runtime,
    counter: &Option<HeldCounter>,
    limit: usize,
    after_gc: &mut usize,
) -> AfterCall {
    let at_end = held(runtime, counter);
    let mut now = at_end;
    let over = at_end > limit / 100 * GC_AFTER_CALL_PERCENT;
    if over || at_end > after_gc.saturating_add(GC_GROWTH_BYTES) {
        runtime.run_gc();
        now = held(runtime, counter);
        *after_gc = now;
    }
    AfterCall {
        exhausted: now >= limit / HELD_AFTER_GC_DEN * HELD_AFTER_GC_NUM,
        at_end,
        now,
    }
}
