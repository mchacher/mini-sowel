//! A refused change: the HTTP status and the text Sowel's API gives for it. Business rules of
//! zones and equipments return it, the API turns it into `{"error": <message>}`.

/// A status (400, 404, 409...) and its message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub status: u16,
    pub message: String,
}

impl Failure {
    pub fn new(status: u16, message: impl Into<String>) -> Self {
        Failure {
            status,
            message: message.into(),
        }
    }
}

/// The result of a change.
pub type Outcome<T> = Result<T, Failure>;
