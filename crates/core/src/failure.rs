//! A refused change: the HTTP status and the text Sowel's API gives for it. Business rules of
//! zones and equipments return it, the API turns it into `{"error": <message>}`, or, for the few
//! refusals Sowel gives a code (`FamilyNotApplicable`...), `{"error": <code>, "message": <text>}`.

/// A status (400, 404, 409...) and its message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub status: u16,
    pub message: String,
    /// Sowel's error code, when the refusal has one: the body is then `{"error": code,
    /// "message": message}`.
    pub code: Option<&'static str>,
}

impl Failure {
    pub fn new(status: u16, message: impl Into<String>) -> Self {
        Failure {
            status,
            message: message.into(),
            code: None,
        }
    }

    /// A refusal with Sowel's code (`FamilyNotApplicable`) and its text.
    pub fn coded(status: u16, code: &'static str, message: impl Into<String>) -> Self {
        Failure {
            status,
            message: message.into(),
            code: Some(code),
        }
    }
}

/// The result of a change.
pub type Outcome<T> = Result<T, Failure>;
