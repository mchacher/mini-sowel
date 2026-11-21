//! Sends the orders of the queue through the radio, one at a time, between the radio's polls: the
//! devices of an order are sent in parallel (at most `MAX_COMMANDS` in flight), the answers are
//! collected as they come, and the order is answered when every device has answered or failed.
//! The radio keeps receiving other frames meanwhile.

use std::collections::VecDeque;

use super::{DeviceCommand, OrderQueue, OrderReply, OrderResult};
use crate::radio::{CommandHandle, CommandTarget, MAX_COMMANDS, Radio, RadioTransport};

/// The order being sent.
struct Active {
    reply: Option<OrderReply>,
    /// On the queue's monotonic clock, in milliseconds.
    deadline: u64,
    /// Commands not sent yet, with their place in the order; at most the order's devices.
    pending: VecDeque<(usize, DeviceCommand)>,
    /// At most `MAX_COMMANDS`.
    in_flight: Vec<(CommandHandle, usize)>,
    /// One outcome per device, in the order of the order's commands.
    outcomes: Vec<Option<Result<(), String>>>,
}

impl Active {
    fn finished(&self) -> bool {
        self.pending.is_empty() && self.in_flight.is_empty()
    }

    /// Done when at least one device took it; else the last failure's reason, as Sowel's
    /// `lastError`. An order without a device to reach is done (Sowel skips a missing device).
    fn result(&self) -> OrderResult {
        if self.outcomes.is_empty() || self.outcomes.iter().flatten().any(Result::is_ok) {
            return Ok(());
        }
        let last = self
            .outcomes
            .iter()
            .rev()
            .flatten()
            .find_map(|o| o.clone().err());
        Err(last.unwrap_or_else(|| "device did not answer".to_string()))
    }
}

/// Takes orders from the queue and sends them; the host's radio loop calls `step` after each
/// poll.
#[derive(Default)]
pub struct OrderRunner {
    active: Option<Active>,
}

impl OrderRunner {
    pub fn new() -> Self {
        OrderRunner::default()
    }

    /// True while an order is being sent.
    pub fn busy(&self) -> bool {
        self.active.is_some()
    }

    /// Collects the answers, sends what can be sent, answers a finished order and takes the next
    /// one. `now` is the queue's clock (monotonic milliseconds).
    pub fn step<T: RadioTransport, C: FnMut() -> u64>(
        &mut self,
        radio: &mut Radio<T, C>,
        queue: &OrderQueue,
        now: u64,
    ) {
        if self.active.is_none()
            && let Some(queued) = queue.take(now)
        {
            let commands = queued.request.commands;
            self.active = Some(Active {
                reply: queued.reply,
                deadline: queued.deadline,
                outcomes: vec![None; commands.len()],
                pending: commands.into_iter().enumerate().collect(),
                in_flight: Vec::new(),
            });
        }
        if let Some(active) = &mut self.active {
            for (handle, result) in radio.take_command_results() {
                let Some(at) = active.in_flight.iter().position(|(h, _)| *h == handle) else {
                    continue;
                };
                let (_, place) = active.in_flight.remove(at);
                active.outcomes[place] = Some(result.map_err(|failure| failure.to_string()));
            }
            // Nobody waits any more, or the time is up: what was not sent is not sent.
            let waiter_gone = active.reply.as_ref().is_some_and(OrderReply::waiter_gone);
            if waiter_gone || now >= active.deadline {
                active.pending.clear();
            }
            while active.in_flight.len() < MAX_COMMANDS
                && let Some((place, command)) = active.pending.pop_front()
            {
                let target = CommandTarget {
                    eui64: command.eui64,
                    nwk: command.nwk,
                    endpoint: command.endpoint,
                };
                let remaining = active.deadline.saturating_sub(now);
                match radio.send_command(target, command.command, remaining) {
                    Ok(handle) => active.in_flight.push((handle, place)),
                    Err(error) => {
                        active.outcomes[place] = Some(Err(error.to_string()));
                    }
                }
            }
            if active.finished() {
                let result = active.result();
                if let Some(reply) = active.reply.take() {
                    reply.send(result);
                }
                self.active = None;
                queue.done();
            }
        }
    }
}
