//! Orders (spec 011): an equipment's order alias turned into commands for its Zigbee devices,
//! the queue that carries them from whoever asks (the API, or the core itself) to the radio
//! loop, and the runner that sends them there. No I/O of its own: the radio loop gives the radio,
//! the time comes from the caller.

mod queue;
mod resolve;
mod runner;

pub use queue::{
    Busy, Clock, DEADLINE_MS, MAX_ORDERS, MAX_UNATTENDED, OrderQueue, OrderReply, OrderResult,
    OrderWait, Queued,
};
pub use resolve::{commands_for, prepare};
pub use runner::OrderRunner;

use std::sync::Arc;

use serde::{Serialize, Serializer};
use serde_json::Value;

use crate::devices::Id;
use crate::radio::zcl;

/// Who gave an order (Sowel's `OrderSource`, `src/shared/types.ts` T:1550-1560): only a recipe
/// of the core gives orders itself, so only that kind exists here. An order of the API has none
/// (Sowel's `manual`, `button`, `external`... need an authenticated user or a channel).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "lowercase",
    rename_all_fields = "camelCase"
)]
pub enum OrderSource {
    Recipe {
        instance_id: Id,
        /// The recipe's `RecipeInfo.name`, a text shared with the recipe's entry in the engine.
        #[serde(serialize_with = "shared_text")]
        recipe_name: Arc<str>,
        #[serde(serialize_with = "shared_text")]
        recipe_id: Arc<str>,
    },
}

/// A shared text as the JSON string it is.
fn shared_text<S: Serializer>(text: &Arc<str>, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(text)
}

/// One command for one device of an order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCommand {
    /// The device, most significant byte first.
    pub eui64: [u8; 8],
    /// The node address its record has; `None` when the record has none.
    pub nwk: Option<u16>,
    /// The endpoint the command goes to.
    pub endpoint: u8,
    pub command: zcl::Command,
}

/// An order, validated and resolved, ready for the radio: the commands of the devices bound under
/// its alias (at most the bindings of an equipment).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderRequest {
    pub equipment: Id,
    pub alias: String,
    /// The value as it was given (the API's raw value, a recipe's `"ON"` or `"OFF"`): what the
    /// order's outcome event says.
    pub value: Value,
    /// Who gave it: `None` for an order of the API.
    pub source: Option<OrderSource>,
    pub commands: Vec<DeviceCommand>,
}
