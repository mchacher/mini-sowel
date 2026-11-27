//! The bounds a recipe and the engine share (spec 041, R2): the state text, the listen set, the
//! params depth, the log. The engine and every recipe (a script, or another recipe written against the SDK) read the same
//! constants, so a bound is written once.

/// The longest state text of an instance. Worst case: 512 bytes kept per instance, 16 instances
/// (8 KB); `motion-light`'s three keys take about 110.
pub const MAX_STATE_BYTES: usize = 512;
/// Zones one instance listens to. Worst case: 1 entry of 36 characters per instance.
pub const MAX_LISTEN_ZONES: usize = 1;
/// Equipment aliases one instance listens to (16 lights and 16 buttons of `motion-light`). Worst
/// case: 32 entries of about 80 bytes per instance, 16 instances (40 KB).
pub const MAX_LISTEN_EQUIPMENTS: usize = 32;
/// Device keys one instance listens to. Worst case: 64 entries of about 80 bytes per instance, 16
/// instances (80 KB).
pub const MAX_LISTEN_DEVICES: usize = 64;
/// How deep the params of an instance may nest. Worst case: a parser (the engine's, a script's)
/// goes 8 levels down at most, so a stack never depends on what a person sent.
pub const MAX_PARAMS_DEPTH: usize = 8;
/// The ids `zone_equipment_ids` returns (the core holds at most 64 equipments). Worst case: 64
/// ids of 36 characters, about 3 KB of text.
pub const MAX_ZONE_EQUIPMENT_IDS: usize = 64;
/// A text value in an event or a view is cut at this many bytes (the core's events cut theirs
/// the same way). Worst case: 32 bytes in each of the 8 data bindings of a view (256 bytes).
pub const MAX_VALUE_TEXT_BYTES: usize = 32;
/// Log lines taken from one call. Worst case: 50 lines of `MAX_LOG_LINE_BYTES`, about 10 KB, taken
/// into the instance's log of 20 entries (the older lines go first).
pub const MAX_LOG_LINES_PER_CALL: usize = 50;
/// A log line is cut at this many bytes. Worst case: 200 bytes per line.
pub const MAX_LOG_LINE_BYTES: usize = 200;
