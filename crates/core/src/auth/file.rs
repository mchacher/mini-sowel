//! `auth.json` (R1.4): the account and its password hash, or `{"version":1}` when there is no
//! account. Never the password itself, never a token.

use serde::{Deserialize, Serialize};

use super::{
    Account, Language, MAX_ITERATIONS, MIN_ITERATIONS, PasswordHash, SCHEME, display_name_ok, hex,
    unhex, username_ok,
};
use crate::devices::{Id, iso};

/// The version of the file's layout.
const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Doc {
    version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    user: Option<FileUser>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    password: Option<FilePassword>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FileUser {
    id: Id,
    username: String,
    display_name: String,
    language: String,
    #[serde(with = "iso")]
    created_at: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FilePassword {
    scheme: String,
    iterations: u32,
    salt: String,
    hash: String,
}

/// The account of a file's text, `None` for `{"version":1}`. A text that is not this file's
/// layout, a version other than 1, a user outside R1.1's bounds, a scheme other than
/// `pbkdf2-sha256`, iterations outside 1 000 to 1 000 000, or a salt or hash of another length is
/// an error (never read as "no account", which would open the setup to anyone).
pub fn parse_file(text: &str) -> Result<Option<Account>, String> {
    let doc: Doc = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if doc.version != VERSION {
        return Err(format!("unsupported version {}", doc.version));
    }
    let (user, password) = match (doc.user, doc.password) {
        (None, None) => return Ok(None),
        (Some(user), Some(password)) => (user, password),
        (Some(_), None) => return Err("a user without a password".into()),
        (None, Some(_)) => return Err("a password without a user".into()),
    };
    if !username_ok(&user.username) {
        return Err("invalid username".into());
    }
    if !display_name_ok(&user.display_name) {
        return Err("invalid displayName".into());
    }
    let language = Language::parse(&user.language).ok_or("invalid language")?;
    if password.scheme != SCHEME {
        return Err(format!("unknown scheme {}", password.scheme));
    }
    if !(MIN_ITERATIONS..=MAX_ITERATIONS).contains(&password.iterations) {
        return Err(format!("invalid iterations {}", password.iterations));
    }
    let salt = unhex::<16>(&password.salt).ok_or("invalid salt")?;
    let hash = unhex::<32>(&password.hash).ok_or("invalid hash")?;
    Ok(Some(Account {
        id: user.id,
        username: user.username,
        display_name: user.display_name,
        language,
        created_at: user.created_at,
        password: PasswordHash {
            iterations: password.iterations,
            salt,
            hash,
        },
    }))
}

/// The file's text for an account, or `{"version":1}` for none.
pub fn to_file_json(account: Option<&Account>) -> String {
    let doc = Doc {
        version: VERSION,
        user: account.map(|a| FileUser {
            id: a.id,
            username: a.username.clone(),
            display_name: a.display_name.clone(),
            language: a.language.as_str().to_string(),
            created_at: a.created_at,
        }),
        password: account.map(|a| FilePassword {
            scheme: SCHEME.to_string(),
            iterations: a.password.iterations,
            salt: hex(&a.password.salt),
            hash: hex(&a.password.hash),
        }),
    };
    // Plain strings and numbers: serialising cannot fail. If it did, the empty text is not a file
    // the loader accepts: the next start stops, it never reads "no account".
    serde_json::to_string(&doc).unwrap_or_default()
}
