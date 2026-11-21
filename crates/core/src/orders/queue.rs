//! The queue of orders between whoever asks and the radio loop, with the slot an answer comes
//! back through. Bounded: at most `MAX_ORDERS` orders in the system (waiting or being sent), so
//! the HTTP workers waiting for them never take them all.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::Duration;

use super::OrderRequest;

/// Orders in the system at once (waiting in the queue, plus the one being sent); one more is
/// refused. With 4 HTTP workers, at least 2 stay free for reads.
pub const MAX_ORDERS: usize = 2;
/// An order is answered, and never sent, after this long in the system, in milliseconds.
pub const DEADLINE_MS: u64 = 12_000;
/// What an order that was not sent in time is answered with.
const NOT_SENT: &str = "device did not answer";

/// The queue is full: `MAX_ORDERS` orders are in the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Busy;

/// The outcome of an order: done when at least one device took it, else the reason of the last
/// failure.
pub type OrderResult = Result<(), String>;

/// The slot an answer goes through.
#[derive(Debug, Default)]
struct Slot {
    result: Mutex<Option<OrderResult>>,
    ready: Condvar,
}

/// The side that waits for the answer (an HTTP worker).
#[derive(Debug)]
pub struct OrderWait(Arc<Slot>);

/// The side that gives the answer (the radio loop). Dropped without an answer, it answers that the
/// device did not.
#[derive(Debug)]
pub struct OrderReply(Arc<Slot>);

fn slot() -> (OrderWait, OrderReply) {
    let slot = Arc::new(Slot::default());
    (OrderWait(Arc::clone(&slot)), OrderReply(slot))
}

impl OrderWait {
    /// Waits for the answer at most `timeout`; an answer that does not come is the device not
    /// answering.
    pub fn wait(self, timeout: Duration) -> OrderResult {
        let guard = self.0.result.lock().unwrap_or_else(PoisonError::into_inner);
        let (mut guard, _) = self
            .0
            .ready
            .wait_timeout_while(guard, timeout, |result| result.is_none())
            .unwrap_or_else(PoisonError::into_inner);
        guard.take().unwrap_or_else(|| Err(NOT_SENT.to_string()))
    }
}

impl OrderReply {
    /// Gives the answer; the waiter wakes up.
    pub fn send(self, result: OrderResult) {
        self.set(result);
    }

    fn set(&self, result: OrderResult) {
        let mut guard = self.0.result.lock().unwrap_or_else(PoisonError::into_inner);
        if guard.is_none() {
            *guard = Some(result);
            self.0.ready.notify_all();
        }
    }

    /// True when nobody waits for the answer any more.
    pub fn waiter_gone(&self) -> bool {
        Arc::strong_count(&self.0) == 1
    }
}

impl Drop for OrderReply {
    fn drop(&mut self) {
        self.set(Err(NOT_SENT.to_string()));
    }
}

/// An order taken from the queue.
#[derive(Debug)]
pub struct Queued {
    pub request: OrderRequest,
    /// `None` for an order the core itself gave: nobody waits for it.
    pub reply: Option<OrderReply>,
    /// The time (on the queue's clock) after which it is neither sent nor waited for.
    pub deadline: u64,
}

/// A monotonic clock in milliseconds, given by the host.
pub type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

#[derive(Default)]
struct Inner {
    /// The clock the deadlines of the orders of the API are read on, when the host gave one.
    clock: Option<Clock>,
    /// At most `MAX_ORDERS` with `active`.
    waiting: VecDeque<Queued>,
    /// An order was taken and is not done.
    active: bool,
}

/// The queue: a handle that is cloned to wherever orders are given or taken.
#[derive(Clone, Default)]
pub struct OrderQueue(Arc<Mutex<Inner>>);

impl std::fmt::Debug for OrderQueue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OrderQueue")
            .field("len", &self.len())
            .finish()
    }
}

impl OrderQueue {
    pub fn new() -> Self {
        OrderQueue::default()
    }

    fn inner(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Gives the queue the monotonic clock (milliseconds) its deadlines are counted on: the
    /// wall clock can jump, and the HTTP waiter does not. The runner is given the same clock's
    /// time.
    pub fn set_clock(&self, clock: Clock) {
        self.inner().clock = Some(clock);
    }

    /// The time on the queue's clock; `None` until `set_clock`.
    pub fn now(&self) -> Option<u64> {
        let clock = self.inner().clock.clone();
        clock.map(|c| c())
    }

    /// Puts an order in the queue at `now` (on the queue's clock); its deadline is
    /// `DEADLINE_MS` later. `Err(Busy)` when `MAX_ORDERS` orders are already in the system.
    pub fn push(
        &self,
        request: OrderRequest,
        reply: Option<OrderReply>,
        now: u64,
    ) -> Result<(), Busy> {
        let mut inner = self.inner();
        if inner.waiting.len() + usize::from(inner.active) >= MAX_ORDERS {
            return Err(Busy);
        }
        inner.waiting.push_back(Queued {
            request,
            reply,
            deadline: now.saturating_add(DEADLINE_MS),
        });
        Ok(())
    }

    /// An order for the core's own use (a recipe): nobody waits for the answer.
    pub fn push_unattended(&self, request: OrderRequest, now: u64) -> Result<(), Busy> {
        self.push(request, None, now)
    }

    /// An order for a caller that waits for the answer: the queue and the wait.
    pub fn push_waited(&self, request: OrderRequest, now: u64) -> Result<OrderWait, Busy> {
        let (wait, reply) = slot();
        self.push(request, Some(reply), now)?;
        Ok(wait)
    }

    /// The next order to send at `now`. An order past its deadline, or whose waiter is gone, is
    /// dropped without being sent (its waiter, if any, is told the device did not answer). The
    /// order returned is the active one until `done`.
    pub fn take(&self, now: u64) -> Option<Queued> {
        let mut inner = self.inner();
        if inner.active {
            return None;
        }
        while let Some(queued) = inner.waiting.pop_front() {
            let gone = queued.reply.as_ref().is_some_and(OrderReply::waiter_gone);
            if now >= queued.deadline || gone {
                continue;
            }
            inner.active = true;
            return Some(queued);
        }
        None
    }

    /// The active order is over.
    pub fn done(&self) {
        self.inner().active = false;
    }

    /// Orders in the system: waiting, plus the active one.
    pub fn len(&self) -> usize {
        let inner = self.inner();
        inner.waiting.len() + usize::from(inner.active)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
