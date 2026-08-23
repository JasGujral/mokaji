//! # mokaji-connector-mail — the third sense
//!
//! **B-9: read and classify only.** This connector cannot send, reply, archive, delete or mark
//! anything read. That is not a policy setting; the IMAP layer it sits on exposes `EXAMINE` rather
//! than `SELECT` and offers no store command, so looking at your mail through MOKaji leaves no
//! trace in the client you actually read it in.
//!
//! **PRIV-5 holds because the socket is not here.** `mokaji-net` owns the connection; this crate's
//! manifest names no networking library and CI fails if it ever does. What this crate owns is the
//! TET contract (A-2) and the judgement: which of the things that arrived actually ask something
//! of you.
//!
//! ## One instance per account
//!
//! Work and personal are two connectors with two ids, not one connector with a mode. A-4's
//! content-identity dedupe then does the right thing for free when the same thread reaches both
//! addresses, and A-6's per-connector health means an expired work password degrades the work
//! panel rather than the app.

#![forbid(unsafe_code)]

pub mod classify;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Local, Utc};
use mokaji_core::connector::{
    Capability, Connector, Health, ProviderQuery, RawPayload, StandardQuery,
};
use mokaji_core::error::Stage;
use mokaji_core::model::{AnyRecord, Area, ConnectorId, Kind, Message, PersonRef, Record};
use mokaji_core::version::RECORD_SCHEMA_VERSION;
use mokaji_core::{Error, Result};
use mokaji_net::imap::{Account, Envelope, Imap};
use mokaji_net::{AuditSink, Consent, KillSwitch};
use std::sync::Arc;

/// The task class recorded in the audit log for every mail fetch.
pub const TASK_CLASS: &str = "MailFetch";

/// Reads one mailbox over IMAP.
pub struct MailConnector {
    id: ConnectorId,
    account: Account,
    mailbox: String,
    area: Area,
    kill: Arc<KillSwitch>,
    audit: Arc<dyn AuditSink>,
    /// Addresses that are *you*. Mail you sent to yourself is not a thing asking something of you.
    self_addresses: Vec<String>,
}

impl MailConnector {
    /// Build a connector for one account.
    #[must_use]
    pub fn new(
        id: &str,
        account: Account,
        area: Area,
        kill: Arc<KillSwitch>,
        audit: Arc<dyn AuditSink>,
    ) -> Self {
        let me = account.user.to_lowercase();
        Self {
            id: id.into(),
            account,
            mailbox: "INBOX".into(),
            area,
            kill,
            audit,
            self_addresses: vec![me],
        }
    }

    /// Read a mailbox other than `INBOX`.
    #[must_use]
    pub fn mailbox(mut self, name: &str) -> Self {
        self.mailbox = name.into();
        self
    }

    /// Additional addresses that count as you — aliases, the other account, a group you own.
    #[must_use]
    pub fn also_me(mut self, addresses: &[String]) -> Self {
        self.self_addresses
            .extend(addresses.iter().map(|a| a.to_lowercase()));
        self
    }

    fn err(&self, stage: Stage, message: impl Into<String>) -> Error {
        Error::Stage {
            connector: self.id.clone(),
            stage,
            message: message.into(),
        }
    }
}

/// How far back to look when the query names no window.
///
/// Seven days rather than "everything": a briefing is about what is live, and an inbox connector
/// that reads your whole history is one that takes a minute to answer a question about this
/// morning.
const DEFAULT_DAYS: i64 = 7;

fn days_for(window: Option<&str>) -> i64 {
    match window.unwrap_or("week") {
        "today" => 1,
        "tomorrow" | "week" => 7,
        "month" => 31,
        _ => DEFAULT_DAYS,
    }
}

#[async_trait]
impl Connector for MailConnector {
    fn id(&self) -> ConnectorId {
        self.id.clone()
    }

    fn schema_version(&self) -> u16 {
        RECORD_SCHEMA_VERSION
    }

    fn capabilities(&self) -> &[Capability] {
        // Read only, and only messages. B-9 written as a declaration the router can see, so a
        // panel asking this connector to write gets a typed refusal rather than a surprise.
        &[Capability::Read(Kind::Message)]
    }

    async fn health(&self) -> Health {
        if !self.kill.allowed() {
            return Health::Degraded("outbound traffic is cut".into());
        }
        if self.account.password.is_empty() {
            return Health::Down("no app password set".into());
        }
        match Imap::connect(
            &self.account,
            &Consent::granted(TASK_CLASS),
            &self.kill,
            &self.audit,
        )
        .await
        {
            Ok(c) => {
                let _ = c.logout().await;
                Health::Ok
            }
            Err(e) => Health::Down(e.to_string()),
        }
    }

    fn transform_query(&self, q: &StandardQuery) -> Result<ProviderQuery> {
        if q.kind != Kind::Message {
            return Err(self.err(Stage::TransformQuery, format!("cannot serve {:?}", q.kind)));
        }
        let since = Local::now().date_naive() - Duration::days(days_for(q.window.as_deref()));
        Ok(ProviderQuery(serde_json::json!({
            "mailbox": self.mailbox,
            "since": since.to_string(),
        })))
    }

    async fn extract(&self, pq: ProviderQuery) -> Result<RawPayload> {
        let mailbox = pq.0["mailbox"]
            .as_str()
            .ok_or_else(|| self.err(Stage::Extract, "no mailbox in the provider query"))?
            .to_string();
        let since: chrono::NaiveDate = pq.0["since"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| self.err(Stage::Extract, "no since date in the provider query"))?;

        let mut c = Imap::connect(
            &self.account,
            &Consent::granted(TASK_CLASS),
            &self.kill,
            &self.audit,
        )
        .await
        .map_err(|e| self.err(Stage::Extract, e.to_string()))?;

        let result = async {
            c.examine(&mailbox).await?;
            let uids = c.search_since(since).await?;
            // Newest first, capped. An inbox with four thousand messages in the window is a real
            // thing, and a briefing does not become better by reading all of them.
            let recent: Vec<u32> = uids.iter().rev().take(200).copied().collect();
            c.fetch_envelopes(&recent).await
        }
        .await;

        let envelopes = match result {
            Ok(e) => e,
            Err(e) => {
                let _ = c.logout().await;
                return Err(self.err(Stage::Extract, e.to_string()));
            }
        };
        let _ = c.logout().await;

        serde_json::to_value(
            envelopes
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "uid": e.uid, "from": e.from, "subject": e.subject,
                        "date": e.date, "message_id": e.message_id, "seen": e.seen,
                    })
                })
                .collect::<Vec<_>>(),
        )
        .map(RawPayload)
        .map_err(|e| self.err(Stage::Extract, e.to_string()))
    }

    fn transform_data(&self, raw: RawPayload) -> Result<Vec<AnyRecord>> {
        let rows = raw
            .0
            .as_array()
            .ok_or_else(|| self.err(Stage::TransformData, "expected an array of envelopes"))?;
        let now = Utc::now();
        let mut out = Vec::with_capacity(rows.len());

        for row in rows {
            let env = Envelope {
                uid: u32::try_from(row["uid"].as_u64().unwrap_or_default()).unwrap_or_default(),
                from: row["from"].as_str().unwrap_or_default().to_string(),
                subject: row["subject"].as_str().unwrap_or_default().to_string(),
                date: row["date"].as_str().unwrap_or_default().to_string(),
                message_id: row["message_id"].as_str().unwrap_or_default().to_string(),
                seen: row["seen"].as_bool().unwrap_or_default(),
            };
            let Some(received) = parse_date(&env.date) else {
                // A message whose Date we cannot read has no place on a timeline. Dropping it is
                // better than inventing `now`, which would put last month's mail at the top.
                continue;
            };
            let from = parse_from(&env.from);
            let needs_action = classify::needs_action(&env, &from, &self.self_addresses);
            let source_ref = if env.message_id.is_empty() {
                format!("{}/{}#{}", self.id, self.mailbox, env.uid)
            } else {
                format!("{}/{}#{}", self.id, self.mailbox, env.message_id)
            };

            out.push(AnyRecord::Message(Record {
                schema_version: RECORD_SCHEMA_VERSION,
                id: format!("{}:{}", self.id, source_ref),
                source: self.id.clone(),
                source_ref: source_ref.clone(),
                area: self.area,
                fetched_at: now,
                data: Message {
                    from,
                    subject: env.subject.clone(),
                    // B-9 again, at the record level: no body is ever fetched, so there is no
                    // snippet to give. An empty string is honest; a truncated subject pretending
                    // to be a preview is not.
                    snippet: String::new(),
                    received,
                    needs_action,
                    thread_ref: env.message_id.clone(),
                },
                raw: None,
                extra: serde_json::Map::new(),
            }));
        }
        Ok(out)
    }
}

/// Parse an RFC 5322 `Date` header.
///
/// Written out rather than pulled in because the header is small and every dependency in this part
/// of the tree inherits the ability to be a supply-chain problem. Obsolete two-digit years and
/// named zones (`GMT`, `EST`) are handled, because real mail still contains them.
#[must_use]
pub fn parse_date(s: &str) -> Option<DateTime<Utc>> {
    let cleaned = s.trim();
    if let Ok(d) = DateTime::parse_from_rfc2822(cleaned) {
        return Some(d.with_timezone(&Utc));
    }
    // Some senders emit a trailing comment: `Sat, 22 Aug 2026 09:14:00 +0100 (BST)`.
    if let Some(cut) = cleaned.find(" (") {
        if let Ok(d) = DateTime::parse_from_rfc2822(&cleaned[..cut]) {
            return Some(d.with_timezone(&Utc));
        }
    }
    None
}

/// Pull a name and address out of a `From` header.
#[must_use]
pub fn parse_from(s: &str) -> PersonRef {
    let t = s.trim();
    if let (Some(a), Some(b)) = (t.rfind('<'), t.rfind('>')) {
        if a < b {
            let email = t[a + 1..b].trim().to_lowercase();
            let name = t[..a].trim().trim_matches('"').trim();
            return PersonRef {
                name: (!name.is_empty()).then(|| name.to_string()),
                email: (!email.is_empty()).then_some(email),
            };
        }
    }
    if t.contains('@') {
        return PersonRef {
            name: None,
            email: Some(t.to_lowercase()),
        };
    }
    PersonRef {
        name: (!t.is_empty()).then(|| t.to_string()),
        email: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_headers_split_into_a_name_and_an_address() {
        let p = parse_from("Harbour Office <ops@example.org>");
        assert_eq!(p.name.as_deref(), Some("Harbour Office"));
        assert_eq!(p.email.as_deref(), Some("ops@example.org"));

        let p = parse_from("\"Lighthouse, North\" <north@example.org>");
        assert_eq!(p.name.as_deref(), Some("Lighthouse, North"));

        // A bare address is common from automated senders and must not become a name.
        let p = parse_from("noreply@example.net");
        assert_eq!(p.name, None);
        assert_eq!(p.email.as_deref(), Some("noreply@example.net"));

        // Addresses are lowercased so A-4's content identity does not see two people.
        assert_eq!(
            parse_from("<OPS@Example.ORG>").email.as_deref(),
            Some("ops@example.org")
        );
    }

    #[test]
    fn dates_survive_the_shapes_real_mail_actually_uses() {
        let d = parse_date("Sat, 22 Aug 2026 09:14:00 +0100").expect("rfc2822");
        assert_eq!(d.to_rfc3339(), "2026-08-22T08:14:00+00:00");
        // A trailing zone comment is common and must not cost us the message.
        assert!(parse_date("Sat, 22 Aug 2026 09:14:00 +0100 (BST)").is_some());
        // An unreadable date is None, so the caller drops the record rather than inventing `now`
        // and putting last month's mail at the top of today's briefing.
        assert!(parse_date("sometime last week").is_none());
    }

    #[test]
    fn transform_query_refuses_kinds_it_cannot_serve_and_names_the_stage() {
        let c = MailConnector::new(
            "mail-work",
            Account {
                host: "imap.example.org".into(),
                port: 993,
                user: "keeper@example.org".into(),
                password: "x".into(),
            },
            Area::Work,
            Arc::new(KillSwitch::new()),
            Arc::new(mokaji_net::MemoryAudit::default()),
        );
        let err = c
            .transform_query(&StandardQuery {
                kind: Kind::Task,
                window: None,
                params: serde_json::Map::new(),
            })
            .expect_err("mail cannot serve tasks");
        // A-2: every error names its stage, so "it returned nothing" is never the whole story.
        assert!(matches!(
            err,
            Error::Stage {
                stage: Stage::TransformQuery,
                ..
            }
        ));
    }

    #[test]
    fn envelopes_become_messages_and_unreadable_dates_are_dropped() {
        let c = MailConnector::new(
            "mail-work",
            Account {
                host: "imap.example.org".into(),
                port: 993,
                user: "keeper@example.org".into(),
                password: "x".into(),
            },
            Area::Work,
            Arc::new(KillSwitch::new()),
            Arc::new(mokaji_net::MemoryAudit::default()),
        );
        let raw = RawPayload(serde_json::json!([
            {"uid": 1, "from": "Harbour Office <ops@example.org>",
             "subject": "Fog signal inspection window", "date": "Sat, 22 Aug 2026 09:14:00 +0100",
             "message_id": "abc@example.org", "seen": false},
            {"uid": 2, "from": "Tenders <tenders@example.net>", "subject": "Tide survey",
             "date": "who knows", "message_id": "def@example.net", "seen": true}
        ]));
        let recs = c.transform_data(raw).expect("transform");
        assert_eq!(recs.len(), 1);
        match &recs[0] {
            AnyRecord::Message(m) => {
                assert_eq!(m.source, "mail-work");
                assert_eq!(m.area, Area::Work);
                assert_eq!(m.data.subject, "Fog signal inspection window");
                // B-9: no body is fetched, so there is no snippet to give.
                assert_eq!(m.data.snippet, "");
                assert_eq!(m.source_ref, "mail-work/INBOX#abc@example.org");
            }
            other => panic!("expected a message, got {other:?}"),
        }
    }

    #[test]
    fn capabilities_declare_read_only_so_a_write_is_refused_before_it_is_attempted() {
        let c = MailConnector::new(
            "mail-personal",
            Account {
                host: "imap.example.org".into(),
                port: 993,
                user: "k@example.org".into(),
                password: "x".into(),
            },
            Area::Personal,
            Arc::new(KillSwitch::new()),
            Arc::new(mokaji_net::MemoryAudit::default()),
        );
        assert_eq!(c.capabilities(), &[Capability::Read(Kind::Message)]);
        assert!(!c
            .capabilities()
            .iter()
            .any(|cap| matches!(cap, Capability::Write(_))));
    }
}
