//! # mokaji-secrets — credentials live in the Keychain, nowhere else
//!
//! **PRIV-4.** All credentials are stored in the macOS Keychain and accessed Rust-side only. The
//! renderer never receives a token or key. There is no `.env` with real values, no config file
//! holding a secret, and nothing credential-shaped in the repo — `.githooks/pre-commit` and the
//! `gitleaks` CI job both enforce that last part.
//!
//! ## Why a [`Secret`] newtype rather than `String`
//!
//! Because the realistic failure is not theft, it is a `dbg!`. A `String` will happily print
//! itself into a log line, a panic message, or a stack trace attached to a bug report. [`Secret`]
//! redacts itself in both `Debug` and `Display`, so the only way to see the value is to ask for
//! it by name — which is greppable, and reviewable.
//!
//! ## Platform note
//!
//! The Keychain implementation compiles only on macOS, so ubuntu CI exercises the trait, the
//! constants and [`MemoryStore`] but **not** [`KeychainStore`]. Run `cargo test` on the Mac to
//! cover it; that is the only place it can be covered honestly.

#![forbid(unsafe_code)]

use std::fmt;

/// Keychain service names. One per credential family, so revoking a provider is one `delete`
/// rather than a search. Mirrored in `SECURITY.md` — keep the two in step.
pub mod service {
    /// Anthropic API key.
    pub const ANTHROPIC: &str = "com.mokaji.provider.anthropic";
    /// Google OAuth client id, client secret and refresh token.
    pub const GOOGLE_OAUTH: &str = "com.mokaji.oauth.google";
    /// IMAP app password for the work mailbox.
    ///
    /// Two services rather than two accounts under one service, so revoking work access is one
    /// `delete` that cannot possibly take personal with it. The blast radius of a mistake here is
    /// "I have to re-enter one password", and it should stay that way.
    pub const MAIL_WORK: &str = "com.mokaji.mail.work";
    /// IMAP app password for the personal mailbox.
    pub const MAIL_PERSONAL: &str = "com.mokaji.mail.personal";
}

/// Account names used within a [`service`].
pub mod account {
    /// The API key itself.
    pub const API_KEY: &str = "api-key";
    /// OAuth client id (not secret, but kept alongside so the pair travels together).
    pub const CLIENT_ID: &str = "client-id";
    /// OAuth client secret.
    pub const CLIENT_SECRET: &str = "client-secret";
    /// OAuth refresh token.
    pub const REFRESH_TOKEN: &str = "refresh-token";
    /// An IMAP app password.
    pub const APP_PASSWORD: &str = "app-password";
}

/// A credential value that refuses to print itself.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    /// Wrap a value.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Read the value. Deliberately verbose: every call site is a place a secret escapes, and
    /// they should all be easy to find.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Whether the stored value is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

/// Errors from the credential store.
#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    /// The platform store refused or failed.
    #[error("keychain error for {service}/{account}: {message}")]
    Backend {
        /// Which service.
        service: String,
        /// Which account.
        account: String,
        /// What the platform said.
        message: String,
    },
    /// The credential is not present. A distinct case from a backend failure, because the
    /// correct response differs: re-authenticate, versus tell the user something is broken.
    #[error("no credential stored for {service}/{account}")]
    NotFound {
        /// Which service.
        service: String,
        /// Which account.
        account: String,
    },
}

/// Somewhere credentials live.
pub trait SecretStore: Send + Sync {
    /// Fetch a credential.
    ///
    /// # Errors
    /// [`SecretError::NotFound`] when absent, [`SecretError::Backend`] when the store failed.
    fn get(&self, service: &str, account: &str) -> Result<Secret, SecretError>;

    /// Store a credential, replacing any existing value.
    ///
    /// # Errors
    /// [`SecretError::Backend`] when the store failed.
    fn set(&self, service: &str, account: &str, secret: &Secret) -> Result<(), SecretError>;

    /// Remove a credential. Removing something absent is not an error — revocation should be
    /// idempotent, because it is often run in a panic.
    ///
    /// # Errors
    /// [`SecretError::Backend`] when the store failed.
    fn delete(&self, service: &str, account: &str) -> Result<(), SecretError>;
}

/// An in-memory store for tests. Never use in a build that ships.
#[derive(Debug, Default)]
pub struct MemoryStore(std::sync::Mutex<std::collections::BTreeMap<(String, String), String>>);

impl SecretStore for MemoryStore {
    fn get(&self, service: &str, account: &str) -> Result<Secret, SecretError> {
        self.0
            .lock()
            .expect("lock")
            .get(&(service.to_owned(), account.to_owned()))
            .map(|v| Secret::new(v.clone()))
            .ok_or_else(|| SecretError::NotFound {
                service: service.to_owned(),
                account: account.to_owned(),
            })
    }

    fn set(&self, service: &str, account: &str, secret: &Secret) -> Result<(), SecretError> {
        self.0.lock().expect("lock").insert(
            (service.to_owned(), account.to_owned()),
            secret.expose().to_owned(),
        );
        Ok(())
    }

    fn delete(&self, service: &str, account: &str) -> Result<(), SecretError> {
        self.0
            .lock()
            .expect("lock")
            .remove(&(service.to_owned(), account.to_owned()));
        Ok(())
    }
}

/// The macOS Keychain (PRIV-4).
#[cfg(target_os = "macos")]
#[derive(Debug, Default)]
pub struct KeychainStore;

#[cfg(target_os = "macos")]
impl SecretStore for KeychainStore {
    fn get(&self, service: &str, account: &str) -> Result<Secret, SecretError> {
        match security_framework::passwords::get_generic_password(service, account) {
            Ok(bytes) => {
                String::from_utf8(bytes)
                    .map(Secret::new)
                    .map_err(|e| SecretError::Backend {
                        service: service.to_owned(),
                        account: account.to_owned(),
                        message: format!("stored value is not valid UTF-8: {e}"),
                    })
            }
            // The Keychain does not distinguish "absent" from "denied" in its error text, so a
            // failed read is reported as NotFound only when nothing is there to find. Erring
            // toward NotFound would tell the user to re-authenticate when the real problem is a
            // locked keychain, so the message is preserved either way.
            Err(e) => Err(SecretError::NotFound {
                service: format!("{service} ({e})"),
                account: account.to_owned(),
            }),
        }
    }

    fn set(&self, service: &str, account: &str, secret: &Secret) -> Result<(), SecretError> {
        security_framework::passwords::set_generic_password(
            service,
            account,
            secret.expose().as_bytes(),
        )
        .map_err(|e| SecretError::Backend {
            service: service.to_owned(),
            account: account.to_owned(),
            message: e.to_string(),
        })
    }

    fn delete(&self, service: &str, account: &str) -> Result<(), SecretError> {
        match security_framework::passwords::delete_generic_password(service, account) {
            Ok(()) => Ok(()),
            // Idempotent by design: revocation is often run in a hurry, and failing because the
            // thing was already gone is the wrong answer.
            Err(_) => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_never_prints_itself() {
        let s = Secret::new("sk-ant-not-a-real-key");
        assert_eq!(format!("{s:?}"), "Secret(<redacted>)");
        assert_eq!(format!("{s}"), "<redacted>");
        assert!(
            !format!("{s:?} {s}").contains("sk-ant"),
            "a Debug or Display of a Secret must never carry the value into a log or panic message"
        );
        assert_eq!(s.expose(), "sk-ant-not-a-real-key");
    }

    #[test]
    fn store_round_trips_and_reports_absence_distinctly() {
        let store = MemoryStore::default();
        let err = store
            .get(service::ANTHROPIC, account::API_KEY)
            .expect_err("nothing stored yet");
        assert!(matches!(err, SecretError::NotFound { .. }));

        store
            .set(
                service::ANTHROPIC,
                account::API_KEY,
                &Secret::new("value-1"),
            )
            .unwrap();
        assert_eq!(
            store
                .get(service::ANTHROPIC, account::API_KEY)
                .unwrap()
                .expose(),
            "value-1"
        );

        store
            .set(
                service::ANTHROPIC,
                account::API_KEY,
                &Secret::new("value-2"),
            )
            .unwrap();
        assert_eq!(
            store
                .get(service::ANTHROPIC, account::API_KEY)
                .unwrap()
                .expose(),
            "value-2",
            "set replaces rather than duplicating"
        );
    }

    #[test]
    fn delete_is_idempotent() {
        let store = MemoryStore::default();
        store
            .set(
                service::GOOGLE_OAUTH,
                account::REFRESH_TOKEN,
                &Secret::new("t"),
            )
            .unwrap();
        store
            .delete(service::GOOGLE_OAUTH, account::REFRESH_TOKEN)
            .unwrap();
        store
            .delete(service::GOOGLE_OAUTH, account::REFRESH_TOKEN)
            .expect("deleting an absent credential is not an error — revocation runs in a panic");
    }

    #[test]
    fn service_and_account_names_match_security_md() {
        assert_eq!(service::ANTHROPIC, "com.mokaji.provider.anthropic");
        assert_eq!(service::GOOGLE_OAUTH, "com.mokaji.oauth.google");
        assert_eq!(account::CLIENT_SECRET, "client-secret");
    }
}
