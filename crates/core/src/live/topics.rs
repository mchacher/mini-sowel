//! The topics of Sowel's WebSocket (`src/api/websocket.ts` W:70-95, `getEventTopic` W:128-153) and
//! the parsing of a client's `subscribe` message (W:403-425).

use serde_json::Value;

/// A topic a client subscribes to. All eleven names of Sowel are accepted; some are never fed
/// (spec 013, out of scope).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Topic {
    Devices,
    Equipments,
    Zones,
    Modes,
    Recipes,
    Calendar,
    MqttPublishers,
    System,
    Logs,
    Activity,
    Energy,
}

impl Topic {
    pub const ALL: [Topic; 11] = [
        Topic::Devices,
        Topic::Equipments,
        Topic::Zones,
        Topic::Modes,
        Topic::Recipes,
        Topic::Calendar,
        Topic::MqttPublishers,
        Topic::System,
        Topic::Logs,
        Topic::Activity,
        Topic::Energy,
    ];

    /// The name a client writes.
    pub fn name(self) -> &'static str {
        match self {
            Topic::Devices => "devices",
            Topic::Equipments => "equipments",
            Topic::Zones => "zones",
            Topic::Modes => "modes",
            Topic::Recipes => "recipes",
            Topic::Calendar => "calendar",
            Topic::MqttPublishers => "mqtt-publishers",
            Topic::System => "system",
            Topic::Logs => "logs",
            Topic::Activity => "activity",
            Topic::Energy => "energy",
        }
    }

    /// The topic of this name, `None` for an unknown one.
    pub fn parse(name: &str) -> Option<Topic> {
        Topic::ALL.into_iter().find(|t| t.name() == name)
    }

    fn bit(self) -> u16 {
        1 << (self as u16)
    }
}

/// The topic of an event type: its prefix before the first dot; what Sowel does not list is
/// `system` (`getEventTopic`).
pub fn topic_of_type(event_type: &str) -> Topic {
    let prefix = event_type.split('.').next().unwrap_or("");
    match prefix {
        "device" => Topic::Devices,
        "equipment" => Topic::Equipments,
        "zone" => Topic::Zones,
        "mode" => Topic::Modes,
        "recipe" => Topic::Recipes,
        "calendar" => Topic::Calendar,
        "mqtt-broker" | "mqtt-publisher" => Topic::MqttPublishers,
        "activity" => Topic::Activity,
        "energy" => Topic::Energy,
        _ => Topic::System,
    }
}

/// A set of topics; `system` is always in a client's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TopicSet(u16);

impl TopicSet {
    /// What a client has before any subscribe: `system` only.
    pub fn system_only() -> TopicSet {
        TopicSet(Topic::System.bit())
    }

    pub fn contains(self, topic: Topic) -> bool {
        self.0 & topic.bit() != 0
    }

    /// The names in the set, in the order of `Topic::ALL`.
    pub fn names(self) -> Vec<&'static str> {
        Topic::ALL
            .into_iter()
            .filter(|t| self.contains(*t))
            .map(Topic::name)
            .collect()
    }
}

/// The topics of a client message: `{"type":"subscribe","topics":[...]}` gives the set (replacing
/// the client's), `system` always in it, unknown names and entries that are not strings ignored.
/// Anything else (text that is not JSON, another `type`, `topics` not an array) is `None`: the
/// message is ignored.
pub fn parse_subscribe(text: &str) -> Option<TopicSet> {
    let message: Value = serde_json::from_str(text).ok()?;
    if message.get("type")?.as_str()? != "subscribe" {
        return None;
    }
    let topics = message.get("topics")?.as_array()?;
    let mut set = TopicSet::system_only();
    for name in topics.iter().filter_map(Value::as_str) {
        if let Some(topic) = Topic::parse(name) {
            set.0 |= topic.bit();
        }
    }
    Some(set)
}
