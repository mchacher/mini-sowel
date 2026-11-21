//! The queue of orders between whoever asks and the radio loop, with the slot an answer comes
//! back through. Bounded: at most `MAX_ORDERS` orders of the API in the system (waiting or being
//! sent), so the HTTP workers waiting for them never take them all; the orders the core gives
//! itself (a recipe's, nobody waits for them) have their own bound, `MAX_UNATTENDED`. The runner
//! takes a waiting order of the API before a waiting order of the core.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::Duration;

use serde_json::Value;

use super::{OrderRequest, OrderSource};
use crate::devices::Id;
use crate::live::{EngineEvent, LiveHandle, Topic};

/// Orders of the API in the system at once (waiting in the queue, plus the one being sent if it
/// is one of them); one more is refused. With 4 HTTP workers, at least 2 stay free for reads.
pub const MAX_ORDERS: usize = 2;
/// Orders the core gave itself (a recipe's) waiting in the queue; one more is dropped. A recipe
/// switches at most 16 lights at a time, so this holds two switchings at once.
pub const MAX_UNATTENDED: usize = 32;
/// An order is answered, and never sent, after this long in the system, in milliseconds.
pub const DEADLINE_MS: u64 = 12_000;
/// What an order that was not sent in time is answered with.
const NOT_SENT: &str = "device did not answer";

/// Builds and sends the outcome event of an order, if a client listens to `equipments`.
fn report(
    live: &Option<LiveHandle>,
    equipment: Id,
    alias: &str,
    value: &Value,
    source: &Option<OrderSource>,
    result: &OrderResult,
) {
    let Some(live) = live else {
        return;
    };
    live.emit(Topic::Equipments, || match result {
        Ok(()) => EngineEvent::EquipmentOrderExecuted {
            equipment_id: equipment,
            order_alias: alias.to_string(),
            value: value.clone(),
            source: source.clone(),
        },
        Err(error) => EngineEvent::EquipmentOrderFailed {
            equipment_id: equipment,
            order_alias: alias.to_string(),
            value: value.clone(),
            error: error.clone(),
            source: source.clone(),
        },
    });
}

/// The queue is full: `MAX_ORDERS` orders of the API (or `MAX_UNATTENDED` of the core) are in the
/// system.
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
    /// Orders of the API: at most `MAX_ORDERS` with `active_http`.
    waiting: VecDeque<Queued>,
    /// Orders of the core: at most `MAX_UNATTENDED`.
    unattended: VecDeque<Queued>,
    /// An order was taken and is not done.
    active: bool,
    /// The order taken is one of the API's.
    active_http: bool,
    /// Where `equipment.order.executed|failed` go (spec 013), when the host listens.
    live: Option<LiveHandle>,
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

    /// Gives the queue the hub the outcomes of orders are told to.
    pub fn set_live(&self, live: LiveHandle) {
        self.inner().live = Some(live);
    }

    /// Tells the clients how an order ended: `equipment.order.executed`, or `failed` with the
    /// reason (Sowel's events of the same names, with the order's source when it has one).
    pub fn report(
        &self,
        equipment: Id,
        alias: &str,
        value: &Value,
        source: &Option<OrderSource>,
        result: &OrderResult,
    ) {
        let live = self.inner().live.clone();
        report(&live, equipment, alias, value, source, result);
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
    /// `DEADLINE_MS` later. An order with a reply slot is the API's: `Err(Busy)` when
    /// `MAX_ORDERS` of them are already in the system. An order without one is the core's:
    /// `Err(Busy)` when `MAX_UNATTENDED` of them are waiting.
    pub fn push(
        &self,
        request: OrderRequest,
        reply: Option<OrderReply>,
        now: u64,
    ) -> Result<(), Busy> {
        let mut inner = self.inner();
        let http = reply.is_some();
        let queued = Queued {
            request,
            reply,
            deadline: now.saturating_add(DEADLINE_MS),
        };
        if http {
            if inner.waiting.len() + usize::from(inner.active && inner.active_http) >= MAX_ORDERS {
                return Err(Busy);
            }
            inner.waiting.push_back(queued);
        } else {
            if inner.unattended.len() >= MAX_UNATTENDED {
                return Err(Busy);
            }
            inner.unattended.push_back(queued);
        }
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

    /// The next order to send at `now`: the first waiting order of the API, else the first of
    /// the core. An order past its deadline, or whose waiter is gone, is dropped without being
    /// sent (its waiter, if any, is told the device did not answer). The order returned is the
    /// active one until `done`.
    pub fn take(&self, now: u64) -> Option<Queued> {
        let mut inner = self.inner();
        if inner.active {
            return None;
        }
        for http in [true, false] {
            loop {
                let next = if http {
                    inner.waiting.pop_front()
                } else {
                    inner.unattended.pop_front()
                };
                let Some(queued) = next else {
                    break;
                };
                let gone = queued.reply.as_ref().is_some_and(OrderReply::waiter_gone);
                if now >= queued.deadline || gone {
                    // Dropped without being sent: the device did not answer.
                    let request = &queued.request;
                    report(
                        &inner.live,
                        request.equipment,
                        &request.alias,
                        &request.value,
                        &request.source,
                        &Err(NOT_SENT.to_string()),
                    );
                    continue;
                }
                inner.active = true;
                inner.active_http = http;
                return Some(queued);
            }
        }
        None
    }

    /// The active order is over.
    pub fn done(&self) {
        self.inner().active = false;
    }

    /// Orders in the system: waiting (the API's and the core's), plus the active one.
    pub fn len(&self) -> usize {
        let inner = self.inner();
        inner.waiting.len() + inner.unattended.len() + usize::from(inner.active)
    }

    /// Orders of the core waiting in the queue.
    pub fn unattended_len(&self) -> usize {
        self.inner().unattended.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
