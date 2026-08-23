//! Mail account configuration — **the part that is not a secret**.
//!
//! Addresses, hosts and mailbox names live in a plain config file; the app password lives in the
//! Keychain and never travels with them. That split is deliberate: you should be able to read this
//! file to find out *which* accounts MOKaji is watching without it being a file worth stealing.
//!
//! **PRIV-4 at the renderer boundary:** the settings screen sends a password *in* exactly once, on
//! save, and never gets one back. What it can read is this config plus a boolean per account.

use serde::{Deserialize, Serialize};

/// One mailbox MOKaji watches.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailAccount {
    /// Stable slot: `"work"` or `"personal"`. Two fixed slots rather than a list, because two
    /// Keychain services with fixed names means revoking one cannot possibly take the other.
    pub slot: String,
    /// The address, which is also the IMAP login for every provider that matters here.
    pub address: String,
    /// IMAP host. Defaults to Gmail's, since that is what both accounts are.
    #[serde(default = "default_host")]
    pub host: String,
    /// Port. 993 implicit TLS; STARTTLS is not supported and will not be.
    #[serde(default = "default_port")]
    pub port: u16,
    /// Which mailbox to read.
    #[serde(default = "default_mailbox")]
    pub mailbox: String,
    /// Whether to read it at all. Off is a first-class state — an account you have configured but
    /// do not want in this morning's briefing should not require deleting the configuration.
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn default_host() -> String {
    "imap.gmail.com".into()
}
fn default_port() -> u16 {
    993
}
fn default_mailbox() -> String {
    "INBOX".into()
}
fn yes() -> bool {
    true
}

/// Everything MOKaji knows about mail, minus the passwords.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MailConfig {
    /// Configured accounts.
    #[serde(default)]
    pub accounts: Vec<MailAccount>,
}

impl MailConfig {
    /// The account in a slot, if configured.
    #[must_use]
    pub fn slot(&self, slot: &str) -> Option<&MailAccount> {
        self.accounts.iter().find(|a| a.slot == slot)
    }
}

/// Where the config lives.
#[must_use]
pub fn config_path() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .map(|h| h.join(".config/mokaji/mail.json"))
}

/// Read it. A missing or unreadable file is an empty config, not an error — mail is optional, and
/// a HUD that refuses to start because you have not set up email is a HUD you stop opening.
#[must_use]
pub fn load() -> MailConfig {
    let Some(p) = config_path() else {
        return MailConfig::default();
    };
    std::fs::read_to_string(p)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Write it.
///
/// # Errors
/// If `$HOME` cannot be located or the file cannot be written.
pub fn save(cfg: &MailConfig) -> Result<(), String> {
    let p = config_path().ok_or("cannot locate $HOME")?;
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    std::fs::write(&p, json).map_err(|e| e.to_string())
}

/// The Keychain service for a slot.
#[must_use]
pub fn service_for(slot: &str) -> Option<&'static str> {
    match slot {
        "work" => Some(mokaji_secrets::service::MAIL_WORK),
        "personal" => Some(mokaji_secrets::service::MAIL_PERSONAL),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_fill_in_so_a_hand_written_config_is_two_lines() {
        let cfg: MailConfig = serde_json::from_str(
            r#"{"accounts":[{"slot":"work","address":"keeper@example.org"}]}"#,
        )
        .expect("parse");
        let a = cfg.slot("work").expect("work");
        assert_eq!(a.host, "imap.gmail.com");
        assert_eq!(a.port, 993);
        assert_eq!(a.mailbox, "INBOX");
        assert!(a.enabled);
    }

    #[test]
    fn an_unknown_slot_has_no_keychain_service_rather_than_a_guessed_one() {
        assert!(service_for("work").is_some());
        assert!(service_for("personal").is_some());
        // Inventing a service name for an unknown slot would silently write a credential to a
        // place nothing ever reads or revokes.
        assert!(service_for("archive").is_none());
    }

    #[test]
    fn the_config_never_has_a_place_to_put_a_password() {
        // A field that does not exist cannot be accidentally serialised into a plain file. This
        // test exists so that adding one has to be a deliberate act with a failing test attached.
        let json = serde_json::to_string(&MailConfig {
            accounts: vec![MailAccount {
                slot: "work".into(),
                address: "keeper@example.org".into(),
                host: default_host(),
                port: 993,
                mailbox: "INBOX".into(),
                enabled: true,
            }],
        })
        .expect("serialize");
        for forbidden in ["password", "secret", "token", "app_password"] {
            assert!(!json.contains(forbidden), "{json}");
        }
    }
}
