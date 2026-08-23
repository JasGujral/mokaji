//! A minimal IMAP client — **the third sense's socket, and the reason it lives here**.
//!
//! Mail is not HTTP, so it cannot reuse [`crate::HttpClient`]. The raw TLS stream has to live
//! somewhere, and the only honest somewhere is this crate: **PRIV-5 says no crate outside
//! `mokaji-net` may name a networking library**, and a `mokaji-connector-mail` that opened its own
//! socket would turn that guarantee back into a promise. So the connector gets a typed API and no
//! ability to reach the network on its own terms.
//!
//! ## Why hand-written rather than an IMAP crate
//!
//! Because the subset actually needed is small — `LOGIN`, `SELECT`, `SEARCH`, `FETCH` of headers —
//! and every dependency added here is one that inherits the ability to open sockets. That trade
//! is worth making once, for reqwest, because HTTP is genuinely large. It is not worth making
//! twice for four commands.
//!
//! ## What this deliberately cannot do
//!
//! **B-9: read and classify only.** There is no `STORE`, no `APPEND`, no `EXPUNGE`, and no way to
//! send a command that is not built by this module. Mail is a sense, not a limb.
//!
//! ## The audit log and the password
//!
//! Every connection is recorded through [`crate::AuditSink`] before the socket opens. The recorded
//! body is **always `None`** — the only request body IMAP would have here is the `LOGIN` line, and
//! an audit log that faithfully records your password is a worse artefact than no audit log at all.

use crate::{AuditEntry, AuditSink, Consent, KillSwitch};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufStream};

/// Where a mailbox lives, and who is asking.
#[derive(Clone)]
pub struct Account {
    /// IMAP host, e.g. `imap.gmail.com`.
    pub host: String,
    /// Port. 993 for implicit TLS, which is the only mode supported — STARTTLS has a downgrade
    /// story and no upside here.
    pub port: u16,
    /// The login name, usually the full address.
    pub user: String,
    /// The password or app password. Never logged, never recorded, never returned.
    pub password: String,
}

impl std::fmt::Debug for Account {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // A `#[derive(Debug)]` here would print the password into the first panic message that
        // ever carried an `Account`. This is the same argument as `Secret`, applied at the point
        // where the value actually travels.
        f.debug_struct("Account")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("user", &self.user)
            .field("password", &"<redacted>")
            .finish()
    }
}

/// One message's envelope. Bodies are never fetched: B-9 is read-and-classify, and a briefing
/// needs to know that a thing arrived and roughly what it is, not what it says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    /// IMAP UID within the mailbox — stable, unlike the sequence number.
    pub uid: u32,
    /// `From` header, raw.
    pub from: String,
    /// `Subject` header, decoded from RFC 2047 where it was encoded.
    pub subject: String,
    /// `Date` header, raw. Parsed by the connector, which owns the model's time rules.
    pub date: String,
    /// `Message-ID`, for a stable `source_ref`.
    pub message_id: String,
    /// Whether the server reported `\Seen`.
    pub seen: bool,
}

/// IMAP errors. Each names what failed rather than what layer failed, because "connection reset"
/// is not something a person can act on and "the server rejected the password" is.
#[derive(Debug, thiserror::Error)]
pub enum ImapError {
    /// The kill switch is cut (PRIV-5).
    #[error("outbound traffic is disabled by the kill switch")]
    KillSwitchEngaged,
    /// TCP or TLS could not be established.
    #[error("could not reach {host}: {message}")]
    Connect {
        /// Destination.
        host: String,
        /// Underlying cause, already stringified — the caller cannot act on the type.
        message: String,
    },
    /// The server said NO or BAD to a command.
    #[error("{command} failed: {response}")]
    Rejected {
        /// Which command.
        command: String,
        /// What the server said. For `LOGIN` this is the server's text, which never echoes the
        /// password back.
        response: String,
    },
    /// The connection ended mid-command.
    #[error("the connection closed during {0}")]
    Closed(String),
    /// A command argument contained something that would change the meaning of the wire protocol.
    #[error("invalid {0}")]
    Invalid(&'static str),
}

type Stream = BufStream<tokio_rustls::client::TlsStream<tokio::net::TcpStream>>;

/// A live, authenticated IMAP connection.
pub struct Imap {
    stream: Stream,
    tag: u32,
    host: String,
}

impl Imap {
    /// Connect, TLS-handshake and `LOGIN`.
    ///
    /// The kill switch is checked **before the DNS lookup**, so a cut switch does not leak the
    /// fact that you were about to check your mail.
    ///
    /// # Errors
    /// [`ImapError::KillSwitchEngaged`], or any failure to connect or authenticate.
    pub async fn connect(
        account: &Account,
        consent: &Consent,
        kill: &Arc<KillSwitch>,
        audit: &Arc<dyn AuditSink>,
    ) -> Result<Self, ImapError> {
        if !kill.allowed() {
            return Err(ImapError::KillSwitchEngaged);
        }
        // Recorded before the socket opens, and with no body: the only body IMAP has here is the
        // LOGIN line, and an audit log that faithfully records your password is worse than none.
        audit.record(AuditEntry {
            at: chrono::Utc::now(),
            task_class: consent.task_class().to_owned(),
            method: "IMAP".into(),
            host: account.host.clone(),
            url: format!("imaps://{}:{}/", account.host, account.port),
            body: None,
        });

        let tcp = tokio::net::TcpStream::connect((account.host.as_str(), account.port))
            .await
            .map_err(|e| ImapError::Connect {
                host: account.host.clone(),
                message: e.to_string(),
            })?;

        let roots = rustls::RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };
        let config = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let name = rustls::pki_types::ServerName::try_from(account.host.clone())
            .map_err(|_| ImapError::Invalid("host name"))?;
        let tls = tokio_rustls::TlsConnector::from(Arc::new(config))
            .connect(name, tcp)
            .await
            .map_err(|e| ImapError::Connect {
                host: account.host.clone(),
                message: e.to_string(),
            })?;

        let mut me = Self {
            stream: BufStream::new(tls),
            tag: 0,
            host: account.host.clone(),
        };
        // The unsolicited greeting, which is not a tagged response and must be consumed before
        // anything else is sent.
        me.read_line("greeting").await?;

        // A literal-safe LOGIN. Quoting is not enough on its own — a password containing a quote
        // or backslash would otherwise change where the argument ends.
        let user = quote(&account.user).ok_or(ImapError::Invalid("user name"))?;
        let pass = quote(&account.password).ok_or(ImapError::Invalid("password"))?;
        me.command("LOGIN", &format!("LOGIN {user} {pass}"), "LOGIN")
            .await?;
        Ok(me)
    }

    /// Select a mailbox read-only. `EXAMINE` rather than `SELECT` on purpose: it cannot clear
    /// `\Recent` or mark anything seen, so looking at your mail through MOKaji leaves no trace in
    /// the client you actually read it in.
    ///
    /// # Errors
    /// If the mailbox does not exist or the server rejects it.
    pub async fn examine(&mut self, mailbox: &str) -> Result<(), ImapError> {
        let m = quote(mailbox).ok_or(ImapError::Invalid("mailbox name"))?;
        self.command("EXAMINE", &format!("EXAMINE {m}"), "EXAMINE")
            .await
            .map(|_| ())
    }

    /// UIDs of messages received since a date, newest last.
    ///
    /// `SINCE` takes a date, not an instant, so this is deliberately coarse — the connector
    /// narrows it against the real timestamps afterwards, where the local-calendar-day rules live.
    ///
    /// # Errors
    /// If the server rejects the search.
    pub async fn search_since(&mut self, since: chrono::NaiveDate) -> Result<Vec<u32>, ImapError> {
        let d = since.format("%d-%b-%Y").to_string();
        let lines = self
            .command("SEARCH", &format!("UID SEARCH SINCE {d}"), "SEARCH")
            .await?;
        let mut uids = Vec::new();
        for l in &lines {
            if let Some(rest) = l.strip_prefix("* SEARCH") {
                uids.extend(
                    rest.split_whitespace()
                        .filter_map(|n| n.parse::<u32>().ok()),
                );
            }
        }
        uids.sort_unstable();
        Ok(uids)
    }

    /// Fetch envelopes for the given UIDs. Headers only — never a body (B-9).
    ///
    /// # Errors
    /// If the server rejects the fetch or the connection ends mid-response.
    pub async fn fetch_envelopes(&mut self, uids: &[u32]) -> Result<Vec<Envelope>, ImapError> {
        if uids.is_empty() {
            return Ok(Vec::new());
        }
        let set = uids
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let lines = self
            .command(
                "FETCH",
                &format!(
                    "UID FETCH {set} (UID FLAGS BODY.PEEK[HEADER.FIELDS (FROM SUBJECT DATE MESSAGE-ID)])"
                ),
                "FETCH",
            )
            .await?;
        Ok(parse_fetch(&lines))
    }

    /// Close politely. A dropped connection works too, but leaving a socket for the server to time
    /// out is rude to a machine that is doing us a favour.
    ///
    /// # Errors
    /// If the connection has already gone.
    pub async fn logout(mut self) -> Result<(), ImapError> {
        let _ = self.command("LOGOUT", "LOGOUT", "LOGOUT").await;
        self.stream
            .shutdown()
            .await
            .map_err(|_| ImapError::Closed("LOGOUT".into()))
    }

    async fn command(
        &mut self,
        name: &str,
        line: &str,
        ctx: &str,
    ) -> Result<Vec<String>, ImapError> {
        self.tag += 1;
        let tag = format!("a{:04}", self.tag);
        let wire = format!("{tag} {line}\r\n");
        self.stream
            .write_all(wire.as_bytes())
            .await
            .map_err(|_| ImapError::Closed(ctx.to_string()))?;
        self.stream
            .flush()
            .await
            .map_err(|_| ImapError::Closed(ctx.to_string()))?;

        let mut out = Vec::new();
        loop {
            let l = self.read_line(ctx).await?;
            if let Some(rest) = l.strip_prefix(&format!("{tag} ")) {
                return match rest.split_whitespace().next() {
                    Some("OK") => Ok(out),
                    _ => Err(ImapError::Rejected {
                        command: name.to_string(),
                        response: rest.trim().to_string(),
                    }),
                };
            }
            out.push(l);
        }
    }

    async fn read_line(&mut self, ctx: &str) -> Result<String, ImapError> {
        let mut buf = Vec::new();
        let n = self
            .stream
            .read_until(b'\n', &mut buf)
            .await
            .map_err(|_| ImapError::Closed(ctx.to_string()))?;
        if n == 0 {
            return Err(ImapError::Closed(ctx.to_string()));
        }
        Ok(String::from_utf8_lossy(&buf).trim_end().to_string())
    }

    /// The host this connection is talking to, for error messages and health.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }
}

/// Quote an IMAP astring, refusing anything that would change where the argument ends.
///
/// Returning `None` rather than escaping-and-hoping: a password containing a newline cannot be
/// sent safely on this protocol at all, and silently mangling it would produce a login failure
/// nobody could explain.
fn quote(s: &str) -> Option<String> {
    if s.contains(['\r', '\n']) || s.is_empty() {
        return None;
    }
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        if c == '"' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    Some(out)
}

/// Pull envelopes out of a `FETCH` response.
///
/// Written as a small state machine over lines rather than a general IMAP parser, because the only
/// response shape this module can produce is the one its own `FETCH` asks for.
fn parse_fetch(lines: &[String]) -> Vec<Envelope> {
    let mut out: Vec<Envelope> = Vec::new();
    let mut cur: Option<Envelope> = None;

    for raw in lines {
        let l = raw.trim_end();
        if l.starts_with("* ") && l.contains("FETCH") {
            if let Some(e) = cur.take() {
                out.push(e);
            }
            cur = Some(Envelope {
                uid: field_u32(l, "UID").unwrap_or_default(),
                from: String::new(),
                subject: String::new(),
                date: String::new(),
                message_id: String::new(),
                // `\Seen` is reported by the server; EXAMINE never sets it.
                seen: l.contains("\\Seen"),
            });
            continue;
        }
        let Some(e) = cur.as_mut() else { continue };
        if let Some(v) = header(l, "From:") {
            e.from = v;
        } else if let Some(v) = header(l, "Subject:") {
            e.subject = decode_rfc2047(&v);
        } else if let Some(v) = header(l, "Date:") {
            e.date = v;
        } else if let Some(v) = header(l, "Message-ID:") {
            e.message_id = v.trim_matches(['<', '>']).to_string();
        }
    }
    if let Some(e) = cur.take() {
        out.push(e);
    }
    // A message with no Message-ID is not addressable, and an un-addressable record breaks the
    // promise that every briefing line cites something (E-8).
    out.retain(|e| !e.message_id.is_empty() || e.uid != 0);
    out
}

fn header(line: &str, name: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    let n = name.to_ascii_lowercase();
    lower
        .starts_with(&n)
        .then(|| line[name.len()..].trim().to_string())
}

/// Read `KEY <number>` out of a FETCH line.
///
/// The parentheses are stripped from the *word* rather than only the value, because the server
/// writes `(UID 4021 FLAGS ...` and a match on the bare key silently finds nothing — a failure
/// mode that produces a plausible-looking envelope with a UID of zero.
fn field_u32(line: &str, key: &str) -> Option<u32> {
    let mut it = line.split_whitespace().map(|w| w.trim_matches(['(', ')']));
    while let Some(w) = it.next() {
        if w.eq_ignore_ascii_case(key) {
            return it.next().and_then(|v| v.parse().ok());
        }
    }
    None
}

/// Decode the RFC 2047 encoded-words that show up in real subject lines.
///
/// Not decoding them means a briefing that reads `=?UTF-8?B?...?=` aloud, which is the kind of
/// detail that decides whether a thing gets used or quietly ignored. Unknown charsets and
/// malformed words are left exactly as they arrived rather than mangled — an undecoded subject is
/// readable; a wrongly-decoded one is not.
fn decode_rfc2047(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find("=?") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("?=") else {
            out.push_str(&rest[start..]);
            return out;
        };
        let word = &after[..end];
        let parts: Vec<&str> = word.splitn(3, '?').collect();
        match (parts.first(), parts.get(1), parts.get(2)) {
            (Some(cs), Some(enc), Some(text)) if cs.eq_ignore_ascii_case("utf-8") => {
                let decoded = match enc.to_ascii_uppercase().as_str() {
                    "B" => base64(text).and_then(|b| String::from_utf8(b).ok()),
                    "Q" => Some(quoted_printable(text)),
                    _ => None,
                };
                match decoded {
                    Some(d) => out.push_str(&d),
                    None => out.push_str(&rest[start..start + 2 + end + 2]),
                }
            }
            _ => out.push_str(&rest[start..start + 2 + end + 2]),
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out.trim().to_string()
}

fn base64(s: &str) -> Option<Vec<u8>> {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut acc: u32 = 0;
    let mut bits = 0u8;
    let mut out = Vec::new();
    for c in s.bytes() {
        if c == b'=' {
            break;
        }
        let v = T.iter().position(|&t| t == c)? as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from((acc >> bits) & 0xFF).ok()?);
        }
    }
    Some(out)
}

fn quoted_printable(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'_' => {
                out.push(b' ');
                i += 1;
            }
            b'=' if i + 2 < b.len() => {
                let hex = std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(v) => {
                        out.push(v);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(b'=');
                        i += 1;
                    }
                }
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_refuses_what_it_cannot_send_rather_than_mangling_it() {
        assert_eq!(quote("plain").as_deref(), Some("\"plain\""));
        assert_eq!(quote(r#"a"b"#).as_deref(), Some(r#""a\"b""#));
        assert_eq!(quote(r"a\b").as_deref(), Some(r#""a\\b""#));
        // A newline in a password would end the command early and send the rest as a new one.
        // Failing loudly beats a login failure nobody can explain.
        assert_eq!(quote("a\nb"), None);
        assert_eq!(quote(""), None);
    }

    #[test]
    fn an_account_never_prints_its_password() {
        let a = Account {
            host: "imap.example.com".into(),
            port: 993,
            user: "keeper@example.com".into(),
            password: "correct-horse-battery-staple".into(),
        };
        let printed = format!("{a:?}");
        assert!(printed.contains("keeper@example.com"));
        assert!(!printed.contains("correct-horse"), "{printed}");
    }

    #[test]
    fn fetch_responses_become_envelopes() {
        let lines: Vec<String> = [
            "* 1 FETCH (UID 4021 FLAGS (\\Seen) BODY[HEADER.FIELDS (FROM SUBJECT DATE MESSAGE-ID)] {120}",
            "From: Harbour Office <ops@example.org>",
            "Subject: Fog signal inspection window",
            "Date: Sat, 22 Aug 2026 09:14:00 +0100",
            "Message-ID: <abc123@example.org>",
            ")",
            "* 2 FETCH (UID 4022 FLAGS () BODY[HEADER.FIELDS (FROM SUBJECT DATE MESSAGE-ID)] {90}",
            "From: Tenders <tenders@example.net>",
            "Subject: =?UTF-8?B?VGlkZSBzdXJ2ZXkg4oCUIHJldmlzZWQ=?=",
            "Date: Sat, 22 Aug 2026 11:02:00 +0100",
            "Message-ID: <def456@example.net>",
            ")",
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect();

        let env = parse_fetch(&lines);
        assert_eq!(env.len(), 2);
        assert_eq!(env[0].uid, 4021);
        assert!(env[0].seen);
        assert_eq!(env[0].subject, "Fog signal inspection window");
        assert_eq!(env[0].message_id, "abc123@example.org");
        assert_eq!(env[1].uid, 4022);
        assert!(!env[1].seen);
        // A briefing that reads "=?UTF-8?B?..." aloud is one that stops being used.
        assert_eq!(env[1].subject, "Tide survey — revised");
    }

    #[test]
    fn an_undecodable_subject_is_left_readable_rather_than_mangled() {
        assert_eq!(
            decode_rfc2047("=?ISO-8859-1?Q?caf=E9?="),
            "=?ISO-8859-1?Q?caf=E9?="
        );
        assert_eq!(decode_rfc2047("=?UTF-8?Q?fog=20signal?="), "fog signal");
        assert_eq!(decode_rfc2047("plain subject"), "plain subject");
        // An unterminated encoded word must not eat the rest of the line.
        assert_eq!(decode_rfc2047("=?UTF-8?B?zzz"), "=?UTF-8?B?zzz");
    }

    #[test]
    fn the_kill_switch_is_checked_before_anything_leaves() {
        let kill = Arc::new(KillSwitch::new());
        kill.cut();
        let memo = Arc::new(crate::MemoryAudit::default());
        let audit: Arc<dyn AuditSink> = memo.clone();
        let account = Account {
            host: "imap.invalid".into(),
            port: 993,
            user: "u".into(),
            password: "p".into(),
        };
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let err = rt
            .block_on(Imap::connect(
                &account,
                &Consent::granted("MailFetch"),
                &kill,
                &audit,
            ))
            .map(|_| ())
            .expect_err("a cut switch must refuse");
        assert!(matches!(err, ImapError::KillSwitchEngaged));
        // Not even an audit entry. A cut switch should leave no trace of an intent it never
        // acted on — and checking before the DNS lookup is what makes that true.
        assert!(memo.entries().is_empty());
    }

    #[test]
    fn a_connection_attempt_is_audited_before_the_socket_opens_and_never_carries_the_password() {
        let kill = Arc::new(KillSwitch::new());
        let memo = Arc::new(crate::MemoryAudit::default());
        let audit: Arc<dyn AuditSink> = memo.clone();
        let account = Account {
            host: "imap.invalid.test".into(),
            port: 993,
            user: "keeper@example.org".into(),
            password: "correct-horse-battery-staple".into(),
        };
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        // The host does not resolve, so this fails — which is the point: the audit entry must
        // already exist, because a crash mid-flight still has to leave evidence.
        let _ = rt.block_on(Imap::connect(
            &account,
            &Consent::granted("MailFetch"),
            &kill,
            &audit,
        ));
        let entries = memo.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].method, "IMAP");
        assert_eq!(entries[0].host, "imap.invalid.test");
        // An audit log that faithfully records your password is worse than no audit log at all.
        assert_eq!(entries[0].body, None);
    }
}
