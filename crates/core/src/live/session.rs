//! The rules of a live session that do not depend on a socket (spec 022, moved from the host's
//! `ws.rs` unchanged): the `Origin` rule, the subprotocol, the timings and the bounds. The host
//! (and, from spec 023, the box's server) moves the bytes; these decide.

/// A client's thread wakes at least this often, in milliseconds: the read timeout of its socket,
/// and the interval of its flushes.
pub const TICK_MS: u64 = 200;
/// A write that takes longer drops the client, in seconds.
pub const WRITE_TIMEOUT_SECONDS: u64 = 5;
/// Pings are sent this often, in seconds, unless `SOLITE_WS_PING_SECONDS` says otherwise (for
/// the tests).
pub const PING_SECONDS: u64 = 30;
/// A client from which nothing arrived for this long is dropped, in seconds, unless
/// `SOLITE_WS_SILENCE_SECONDS` says otherwise (for the tests).
pub const SILENCE_SECONDS: u64 = 60;
/// Incoming messages and frames are refused (close 1009) past this many bytes.
pub const MAX_INCOMING_BYTES: usize = 1024;
/// The development server of the interface (B8), accepted as an origin besides the host's own.
pub const DEV_ORIGINS: [&str; 2] = ["http://localhost:5173", "http://127.0.0.1:5173"];

/// Sowel's origin rule (`src/api/websocket.ts` W:48-68): the `Origin` is absent, or its
/// `host:port` is the `Host` header's; the interface's development server is accepted too.
pub fn origin_allowed(origin: Option<&str>, host: Option<&str>) -> bool {
    let Some(origin) = origin else {
        return true;
    };
    let origin = origin.trim();
    if DEV_ORIGINS.contains(&origin) {
        return true;
    }
    let Some(authority) = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
    else {
        return false;
    };
    let authority = authority.trim_end_matches('/');
    !authority.is_empty()
        && !authority.contains('/')
        && host.is_some_and(|h| h.trim().eq_ignore_ascii_case(authority))
}

/// The subprotocol to echo, from the values of the `Sec-WebSocket-Protocol` headers: the first
/// offered that starts with `bearer.` (the browser drops the socket if it is not echoed). The
/// token is read by `bearer_token`.
pub fn pick_subprotocol<'a>(lines: impl IntoIterator<Item = &'a str>) -> Option<String> {
    lines
        .into_iter()
        .flat_map(|line| line.split(','))
        .map(str::trim)
        .find(|p| p.starts_with("bearer."))
        .map(str::to_string)
}

/// The token of a `/ws` upgrade (spec 029, R5.1, D49): the `Authorization: Bearer <token>` header
/// if there is one, else the first offered subprotocol `bearer.<token>` other than `bearer.none`
/// (the interface offers `bearer.none` first, for the box's server, which echoes a fixed
/// subprotocol). `None` when there is none: the socket is closed 4001.
pub fn bearer_token<'a>(
    lines: impl IntoIterator<Item = &'a str>,
    authorization: Option<&str>,
) -> Option<String> {
    if let Some(token) = authorization
        .and_then(|header| header.strip_prefix("Bearer "))
        .filter(|token| !token.is_empty())
    {
        return Some(token.to_string());
    }
    lines
        .into_iter()
        .flat_map(|line| line.split(','))
        .map(str::trim)
        .filter_map(|p| p.strip_prefix("bearer."))
        .find(|token| !token.is_empty() && *token != "none")
        .map(str::to_string)
}

/// The close code and reason of a socket that came without a token (R5.3).
pub const CLOSE_UNAUTHENTICATED: u16 = 4001;
/// The reason when no token was offered.
pub const REASON_AUTHENTICATION_REQUIRED: &str = "Authentication required";
/// The reason when the token is not a live access token.
pub const REASON_INVALID_TOKEN: &str = "Invalid token";

/// The reason text of a close frame.
pub fn close_reason(code: u16) -> &'static str {
    match code {
        1001 => "Going away",
        1002 => "Protocol error",
        1007 => "Invalid payload",
        1009 => "Message too big",
        1013 => "Try again later",
        _ => "",
    }
}
