//! Live events (spec 013): Sowel's WebSocket `/ws`, without a socket. The core decides what a
//! client receives: its topics, the events waiting for it with Sowel's dedup, the bounds, the
//! text of a batch. The host owns the sockets: its thread for a client takes the batch every
//! 200 ms (`begin_batch`, `pop`) and writes it. Events are built by the home where the change
//! happens, through `LiveHub::emit`, which builds nothing when no connected client listens to
//! the topic, and serialises each event once for all the clients that get it.
//!
//! Lock order, for the whole program: home, then orders' queue, then this hub. The hub never
//! calls out while it is locked, so a client's thread, which takes the hub only, cannot hold up
//! the home.

mod events;
mod topics;

pub use events::EngineEvent;
pub use topics::{Topic, TopicSet, parse_subscribe, topic_of_type};

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// Clients connected at once; one more is refused (the host answers 503).
pub const MAX_CLIENTS: usize = 4;
/// Events waiting for one client, at most (after dedup).
pub const MAX_QUEUED_EVENTS: usize = 256;
/// The bytes one client's waiting events take, at most: their JSON, their dedup key, and
/// `ENTRY_OVERHEAD` for each entry. One past it, the client is closed with 1013.
pub const MAX_QUEUED_BYTES: usize = 8 * 1024;
/// What a queue entry costs besides its text: its slot (two shared pointers, 32 bytes) and the
/// headers of the two shared allocations, rounded up.
pub const ENTRY_OVERHEAD: usize = 64;
/// The close code of a client whose queue overflowed ("try again later").
pub const CLOSE_TRY_AGAIN_LATER: u16 = 1013;
// Worst case of these bounds, for each client (the host's `ws.rs` sets the buffers): its queue,
// 8 KB; the event its thread is writing, 8 KB at most (it left the queue, which can fill again
// while a blocked write waits; the frames share its text, nothing is copied); the library's
// write buffer, 1 KB to flush plus one 1 KB fragment and its header, so 5 KB of capacity at
// worst; its read buffer, 2 KB to 4 KB at worst. That is about 25 KB, and 4 clients make about
// 100 KB. A batch is never built: the host takes the events one at a time (`pop`). The kernel's
// send buffer (16 KB each) is not on this heap.

/// The hub every part of the home shares.
pub type LiveHandle = Arc<LiveHub>;

/// A client's number: never reused.
pub type ClientId = u64;

/// An event serialised once, with its dedup key, shared by the queues it is in.
struct Queued {
    key: Option<Arc<str>>,
    json: Arc<str>,
}

impl Queued {
    fn cost(&self) -> usize {
        self.json.len() + self.key.as_ref().map_or(0, |k| k.len()) + ENTRY_OVERHEAD
    }
}

struct Client {
    id: ClientId,
    topics: TopicSet,
    /// At most `MAX_QUEUED_EVENTS` entries and `MAX_QUEUED_BYTES`.
    queue: VecDeque<Queued>,
    bytes: usize,
    /// The first entries of the queue belong to the batch being sent: they are not deduped any
    /// more (the host has begun to write them).
    batch_left: usize,
    /// Set when the client must be closed with this code.
    close: Option<u16>,
}

struct Hub {
    /// At most `MAX_CLIENTS`.
    clients: Vec<Client>,
    next_id: ClientId,
    /// Events built since the start (a test counts them: with no listener it stays 0). Kept
    /// under the lock: the ESP32's Xtensa core has no 64-bit atomics.
    built: u64,
}

/// The clients' queues.
pub struct LiveHub {
    inner: Mutex<Hub>,
}

impl std::fmt::Debug for LiveHub {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveHub")
            .field("clients", &self.client_count())
            .finish()
    }
}

impl Default for LiveHub {
    fn default() -> Self {
        LiveHub::new()
    }
}

impl LiveHub {
    pub fn new() -> Self {
        LiveHub {
            inner: Mutex::new(Hub {
                clients: Vec::with_capacity(MAX_CLIENTS),
                next_id: 1,
                built: 0,
            }),
        }
    }

    /// A new hub to share.
    pub fn handle() -> LiveHandle {
        Arc::new(LiveHub::new())
    }

    fn hub(&self) -> MutexGuard<'_, Hub> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    // ---- Clients ----

    /// Adds a client, listening to `system` only; `None` when `MAX_CLIENTS` are connected.
    pub fn connect(&self) -> Option<ClientId> {
        let mut hub = self.hub();
        if hub.clients.len() >= MAX_CLIENTS {
            return None;
        }
        let id = hub.next_id;
        hub.next_id += 1;
        hub.clients.push(Client {
            id,
            topics: TopicSet::system_only(),
            queue: VecDeque::new(),
            bytes: 0,
            batch_left: 0,
            close: None,
        });
        Some(id)
    }

    /// Removes a client and frees its queue; its slot is free again.
    pub fn disconnect(&self, id: ClientId) {
        self.hub().clients.retain(|c| c.id != id);
    }

    pub fn client_count(&self) -> usize {
        self.hub().clients.len()
    }

    /// Applies a client's text message: a `subscribe` replaces its topics. True when it did;
    /// anything else is ignored.
    pub fn subscribe(&self, id: ClientId, text: &str) -> bool {
        let Some(topics) = parse_subscribe(text) else {
            return false;
        };
        let mut hub = self.hub();
        match hub.clients.iter_mut().find(|c| c.id == id) {
            Some(client) => {
                client.topics = topics;
                true
            }
            None => false,
        }
    }

    /// The client's topics.
    pub fn topics(&self, id: ClientId) -> Option<TopicSet> {
        self.hub()
            .clients
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.topics)
    }

    // ---- Events ----

    /// True when a connected client (one not being closed) listens to the topic.
    pub fn wants(&self, topic: Topic) -> bool {
        self.hub()
            .clients
            .iter()
            .any(|c| c.close.is_none() && c.topics.contains(topic))
    }

    /// Events built so far.
    pub fn events_built(&self) -> u64 {
        self.hub().built
    }

    /// Builds the event and queues it for the clients that listen to its topic, only if there is
    /// one: with no listener `build` is not called.
    pub fn emit(&self, topic: Topic, build: impl FnOnce() -> EngineEvent) {
        let mut hub = self.hub();
        if !hub
            .clients
            .iter()
            .any(|c| c.close.is_none() && c.topics.contains(topic))
        {
            return;
        }
        hub.built += 1;
        let event = build();
        let topic = event.topic();
        let json: Arc<str> = match serde_json::to_string(&event) {
            Ok(text) => text.into(),
            Err(e) => {
                log::error!("live: cannot serialise {}: {e}", event.type_name());
                return;
            }
        };
        let key: Option<Arc<str>> = event.dedup_key().map(Into::into);
        drop(event);
        for client in &mut hub.clients {
            if client.close.is_some() || !client.topics.contains(topic) {
                continue;
            }
            client.push(key.clone(), Arc::clone(&json));
        }
    }

    // ---- What the host's thread does for a client ----

    /// The close code the client must be closed with, if its queue overflowed.
    pub fn close_request(&self, id: ClientId) -> Option<u16> {
        self.hub()
            .clients
            .iter()
            .find(|c| c.id == id)
            .and_then(|c| c.close)
    }

    /// Starts a batch: the events waiting now, which `pop` gives one by one; their number.
    /// Events that arrive meanwhile wait for the next batch, and do not dedup the ones being sent.
    pub fn begin_batch(&self, id: ClientId) -> usize {
        let mut hub = self.hub();
        match hub.clients.iter_mut().find(|c| c.id == id) {
            Some(client) => {
                client.batch_left = client.queue.len();
                client.batch_left
            }
            None => 0,
        }
    }

    /// The next event of the batch begun, its JSON; `None` at the end.
    pub fn pop(&self, id: ClientId) -> Option<Arc<str>> {
        let mut hub = self.hub();
        let client = hub.clients.iter_mut().find(|c| c.id == id)?;
        if client.batch_left == 0 {
            return None;
        }
        let queued = client.queue.pop_front()?;
        client.batch_left -= 1;
        client.bytes = client.bytes.saturating_sub(queued.cost());
        Some(queued.json)
    }

    /// The whole batch as one JSON array text, `None` when no event waits. The host does not
    /// use it (it writes the events one by one); it is the batch's definition, for tests.
    pub fn take_batch(&self, id: ClientId) -> Option<String> {
        let count = self.begin_batch(id);
        if count == 0 {
            return None;
        }
        let mut text = String::from("[");
        for n in 0..count {
            let json = self.pop(id)?;
            if n > 0 {
                text.push(',');
            }
            text.push_str(&json);
        }
        text.push(']');
        Some(text)
    }

    /// Events and bytes waiting for the client (tests, the memory ceiling).
    pub fn queued(&self, id: ClientId) -> (usize, usize) {
        self.hub()
            .clients
            .iter()
            .find(|c| c.id == id)
            .map_or((0, 0), |c| (c.queue.len(), c.bytes))
    }
}

impl Client {
    /// Queues an event; an event with a dedup key replaces the one with the same key that is
    /// still waiting (it moves to the last position). Past the bounds the client is marked to be
    /// closed and its queue freed.
    fn push(&mut self, key: Option<Arc<str>>, json: Arc<str>) {
        if let Some(key) = &key {
            let batch = self.batch_left;
            let same = self
                .queue
                .iter()
                .skip(batch)
                .position(|q| q.key.as_deref() == Some(&**key));
            if let Some(at) = same
                && let Some(old) = self.queue.remove(batch + at)
            {
                self.bytes = self.bytes.saturating_sub(old.cost());
            }
        }
        let queued = Queued { key, json };
        let cost = queued.cost();
        if self.queue.len() >= MAX_QUEUED_EVENTS || self.bytes + cost > MAX_QUEUED_BYTES {
            log::warn!("live: client {} is too slow, closing it", self.id);
            self.queue = VecDeque::new();
            self.bytes = 0;
            self.batch_left = 0;
            self.close = Some(CLOSE_TRY_AGAIN_LATER);
            return;
        }
        self.bytes += cost;
        self.queue.push_back(queued);
    }
}
