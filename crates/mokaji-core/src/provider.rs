//! The brain — **DEC-3, FR-E, §7d**.
//!
//! ```text
//! policy[TaskClass] -> Tier ; Cloud requires Consent(TaskClass) ; Cloud failure -> Local
//! ```
//!
//! **X-1 / PRIV-1 / PRIV-2.** Local data, local senses, opt-in cloud cognition. Audio never
//! reaches a provider — the audio crate has no network dependency at all, and that is structural
//! rather than a matter of care. Vault contents, the index and the audit log leave only as
//! explicit, consented, audit-logged model context.
//!
//! **E-2 pins briefing assembly local.** The morning briefing is the core daily loop, and §12's
//! anti-requirements forbid a cloud dependency there. A machine with the network cable out must
//! still be able to tell you about your day.
//!
//! **E-8: citations are not decoration.** A plan whose claims cannot be traced to a record id is
//! indistinguishable from a plausible invention, and this system's whole argument is that it knows
//! things about *your* life rather than about life in general.

use crate::model::AnyRecord;
use async_trait::async_trait;
use std::collections::BTreeMap;

/// Where a completion runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    /// On this machine.
    Local,
    /// Off this machine, with consent and an audit entry.
    Cloud,
}

/// What kind of thinking is being asked for. The policy table is keyed by this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskClass {
    /// Tidying a raw transcript. Small, local.
    TranscriptCleanup,
    /// Parsing an utterance into an intent. Small and constrained — what small models do well.
    IntentParse,
    /// Pulling entities out of text.
    EntityExtraction,
    /// Bucketing something into a known set.
    Classification,
    /// A sentence or two of drafting.
    ShortDraft,
    /// Assembling the morning briefing. **Pinned local by E-2 and §12.**
    BriefingAssembly,
    /// Multi-step planning.
    Planning,
    /// Open-ended synthesis.
    Synthesis,
    /// Reasoning over a long context.
    LongContext,
    /// Driving a chain of tools.
    ToolChain,
}

impl TaskClass {
    /// Every class, so a settings screen can enumerate them without a hand-written list going stale.
    #[must_use]
    pub fn all() -> &'static [TaskClass] {
        use TaskClass::{
            BriefingAssembly, Classification, EntityExtraction, IntentParse, LongContext, Planning,
            ShortDraft, Synthesis, ToolChain, TranscriptCleanup,
        };
        &[
            TranscriptCleanup,
            IntentParse,
            EntityExtraction,
            Classification,
            ShortDraft,
            BriefingAssembly,
            Planning,
            Synthesis,
            LongContext,
            ToolChain,
        ]
    }

    /// **E-2.** Whether this class may *ever* leave the machine.
    ///
    /// `BriefingAssembly` is false and is not configurable. Everything else about the policy table
    /// is a preference; this one is a promise, and a promise you can switch off in a settings pane
    /// is a preference wearing a promise's clothes.
    #[must_use]
    pub fn may_leave_device(self) -> bool {
        !matches!(self, Self::BriefingAssembly)
    }
}

/// **E-2 — the policy table.** Task class in, tier out.
#[derive(Debug, Clone)]
pub struct Policy(BTreeMap<TaskClass, Tier>);

impl Default for Policy {
    /// The defaults from the requirements: small, constrained, private work stays local; hard
    /// reasoning may go to the cloud with consent.
    fn default() -> Self {
        use TaskClass::{
            BriefingAssembly, Classification, EntityExtraction, IntentParse, LongContext, Planning,
            ShortDraft, Synthesis, ToolChain, TranscriptCleanup,
        };
        let mut m = BTreeMap::new();
        for c in [
            TranscriptCleanup,
            IntentParse,
            EntityExtraction,
            Classification,
            ShortDraft,
            BriefingAssembly,
        ] {
            m.insert(c, Tier::Local);
        }
        for c in [Planning, Synthesis, LongContext, ToolChain] {
            m.insert(c, Tier::Cloud);
        }
        Self(m)
    }
}

impl Policy {
    /// Which tier a class routes to.
    #[must_use]
    pub fn tier(&self, class: TaskClass) -> Tier {
        // A class pinned local by E-2 cannot be routed to cloud even if the table says so — the
        // table is configuration and E-2 is not.
        if !class.may_leave_device() {
            return Tier::Local;
        }
        self.0.get(&class).copied().unwrap_or(Tier::Local)
    }

    /// Change where a class runs.
    ///
    /// # Errors
    /// If the class is pinned local by E-2. Refusing here rather than silently ignoring the change
    /// is the difference between a promise and a setting.
    pub fn set(&mut self, class: TaskClass, tier: Tier) -> Result<(), PolicyError> {
        if tier == Tier::Cloud && !class.may_leave_device() {
            return Err(PolicyError::PinnedLocal(class));
        }
        self.0.insert(class, tier);
        Ok(())
    }
}

/// Why a policy change was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PolicyError {
    /// E-2 / §12: the core daily loop must not depend on the network.
    #[error("{0:?} is pinned local — the daily loop must work with the network cable out")]
    PinnedLocal(TaskClass),
}

/// **E-3 — per-class consent**, granted once and revocable.
#[derive(Debug, Clone, Default)]
pub struct Consents(BTreeMap<TaskClass, bool>);

impl Consents {
    /// Whether this class may currently leave the machine.
    #[must_use]
    pub fn granted(&self, class: TaskClass) -> bool {
        class.may_leave_device() && self.0.get(&class).copied().unwrap_or(false)
    }

    /// Grant consent for a class.
    ///
    /// # Errors
    /// If the class is pinned local.
    pub fn grant(&mut self, class: TaskClass) -> Result<(), PolicyError> {
        if !class.may_leave_device() {
            return Err(PolicyError::PinnedLocal(class));
        }
        self.0.insert(class, true);
        Ok(())
    }

    /// Revoke it. Always allowed, and immediate — a consent you cannot withdraw is not consent.
    pub fn revoke(&mut self, class: TaskClass) {
        self.0.insert(class, false);
    }
}

/// A completion request.
#[derive(Debug, Clone)]
pub struct Completion {
    /// What is being asked.
    pub class: TaskClass,
    /// The prompt.
    pub prompt: String,
    /// **E-7.** The records the Deck is currently showing, so "explain that number" is answerable.
    pub context: Vec<AnyRecord>,
}

/// **E-8 — a claim and where it came from.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Citation {
    /// The record id backing the claim.
    pub record_id: String,
    /// Which connector it came from.
    pub source: String,
    /// The pointer home — `path#Lnn` for a vault record.
    pub source_ref: String,
}

/// What a provider returned.
#[derive(Debug, Clone)]
pub struct Answer {
    /// The text.
    pub text: String,
    /// Where each claim came from (E-8).
    pub citations: Vec<Citation>,
    /// Which tier actually answered — not which one was asked for. E-6 means those differ.
    pub tier: Tier,
    /// Set when the cloud was wanted and the local tier answered instead (E-6).
    pub degraded: Option<String>,
}

/// A source of completions (§7d).
#[async_trait]
pub trait ModelProvider: Send + Sync {
    /// Stable id, e.g. `"anthropic"`, `"local"`.
    fn id(&self) -> &str;

    /// Local or cloud.
    fn tier(&self) -> Tier;

    /// Whether this provider can currently answer — RAM for a local model (E-9), reachability for
    /// a cloud one.
    async fn ready(&self) -> bool {
        true
    }

    /// Run a completion.
    ///
    /// # Errors
    /// Any provider failure. The router turns a cloud failure into a local answer (E-6).
    async fn complete(&self, req: &Completion) -> Result<Answer, String>;
}

/// Routes a completion to a tier, honouring the policy, consent, and E-6's degradation.
pub struct Brain {
    local: Box<dyn ModelProvider>,
    cloud: Option<Box<dyn ModelProvider>>,
    policy: Policy,
    consents: Consents,
}

/// Why a completion could not run.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BrainError {
    /// Both tiers failed. E-6 promises never a dead end, and this is the case where that promise
    /// cannot be kept — so it says so loudly rather than returning an empty answer.
    #[error("no provider could answer: local said `{local}`{}", cloud.as_ref().map(|c| format!(", cloud said `{c}`")).unwrap_or_default())]
    NoProvider {
        /// What the local provider said.
        local: String,
        /// What the cloud provider said, if one was tried.
        cloud: Option<String>,
    },
}

impl Brain {
    /// A brain with a local provider and optionally a cloud one.
    #[must_use]
    pub fn new(local: Box<dyn ModelProvider>, cloud: Option<Box<dyn ModelProvider>>) -> Self {
        Self {
            local,
            cloud,
            policy: Policy::default(),
            consents: Consents::default(),
        }
    }

    /// The policy table.
    #[must_use]
    pub fn policy(&self) -> &Policy {
        &self.policy
    }

    /// Mutable policy table.
    pub fn policy_mut(&mut self) -> &mut Policy {
        &mut self.policy
    }

    /// The consent record.
    #[must_use]
    pub fn consents(&self) -> &Consents {
        &self.consents
    }

    /// Mutable consent record.
    pub fn consents_mut(&mut self) -> &mut Consents {
        &mut self.consents
    }

    /// Where a request *would* go, and why. Useful for the in-UI indicator (E-3) before anything
    /// is sent.
    #[must_use]
    pub fn route(&self, class: TaskClass) -> (Tier, &'static str) {
        if !class.may_leave_device() {
            return (
                Tier::Local,
                "pinned local — the daily loop must work offline",
            );
        }
        match self.policy.tier(class) {
            Tier::Local => (Tier::Local, "policy routes this class local"),
            Tier::Cloud if !self.consents.granted(class) => {
                (Tier::Local, "cloud not consented for this class")
            }
            Tier::Cloud if self.cloud.is_none() => (Tier::Local, "no cloud provider configured"),
            Tier::Cloud => (Tier::Cloud, "policy routes this class cloud, with consent"),
        }
    }

    /// Answer a completion.
    ///
    /// # Errors
    /// [`BrainError::NoProvider`] only when both tiers fail.
    pub async fn complete(&self, req: &Completion) -> Result<Answer, BrainError> {
        let (tier, _why) = self.route(req.class);

        if tier == Tier::Cloud {
            if let Some(cloud) = &self.cloud {
                match cloud.complete(req).await {
                    Ok(a) => return Ok(a),
                    Err(e) => {
                        // E-6: never a dead end. Degrade, and SAY SO — a silently-local answer to a
                        // question you consented to send is a different product than the one you
                        // agreed to.
                        return match self.local.complete(req).await {
                            Ok(mut a) => {
                                a.degraded =
                                    Some(format!("cloud unavailable ({e}) — answered locally"));
                                a.tier = Tier::Local;
                                Ok(a)
                            }
                            Err(le) => Err(BrainError::NoProvider {
                                local: le,
                                cloud: Some(e),
                            }),
                        };
                    }
                }
            }
        }

        self.local
            .complete(req)
            .await
            .map(|mut a| {
                a.tier = Tier::Local;
                a
            })
            .map_err(|e| BrainError::NoProvider {
                local: e,
                cloud: None,
            })
    }
}

/// Build citations for every record that was put in front of the model (E-8).
///
/// Deliberately mechanical: a citation is a fact about what the model was *given*, not a claim the
/// model makes about itself. Asking a model to cite its own sources is asking it to be honest about
/// something it cannot check.
#[must_use]
pub fn citations_for(records: &[AnyRecord]) -> Vec<Citation> {
    records
        .iter()
        .map(|r| {
            let (id, source, source_ref) = record_ids(r);
            Citation {
                record_id: id,
                source,
                source_ref,
            }
        })
        .collect()
}

fn record_ids(r: &AnyRecord) -> (String, String, String) {
    macro_rules! ids {
        ($x:expr) => {
            ($x.id.clone(), $x.source.clone(), $x.source_ref.clone())
        };
    }
    match r {
        AnyRecord::Task(x) => ids!(x),
        AnyRecord::Event(x) => ids!(x),
        AnyRecord::Chaser(x) => ids!(x),
        AnyRecord::Note(x) => ids!(x),
        AnyRecord::Person(x) => ids!(x),
        AnyRecord::Metric(x) => ids!(x),
        AnyRecord::Message(x) => ids!(x),
        AnyRecord::Goal(x) => ids!(x),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Area, Record, Task};
    use chrono::{DateTime, Utc};

    struct Fake {
        id: &'static str,
        tier: Tier,
        fail: Option<&'static str>,
    }

    #[async_trait]
    impl ModelProvider for Fake {
        fn id(&self) -> &str {
            self.id
        }
        fn tier(&self) -> Tier {
            self.tier
        }
        async fn complete(&self, req: &Completion) -> Result<Answer, String> {
            match self.fail {
                Some(e) => Err(e.to_string()),
                None => Ok(Answer {
                    text: format!("{} answered {:?}", self.id, req.class),
                    citations: citations_for(&req.context),
                    tier: self.tier,
                    degraded: None,
                }),
            }
        }
    }

    fn local() -> Box<dyn ModelProvider> {
        Box::new(Fake {
            id: "local",
            tier: Tier::Local,
            fail: None,
        })
    }
    fn cloud() -> Box<dyn ModelProvider> {
        Box::new(Fake {
            id: "cloud",
            tier: Tier::Cloud,
            fail: None,
        })
    }
    fn broken_cloud() -> Box<dyn ModelProvider> {
        Box::new(Fake {
            id: "cloud",
            tier: Tier::Cloud,
            fail: Some("no network"),
        })
    }

    fn req(class: TaskClass) -> Completion {
        Completion {
            class,
            prompt: "plan my day".into(),
            context: vec![task_record()],
        }
    }

    fn task_record() -> AnyRecord {
        AnyRecord::Task(Record {
            schema_version: 1,
            id: "vault:task:notes/x.md#L4".into(),
            source: "vault".into(),
            source_ref: "notes/x.md#L4".into(),
            area: Area::Personal,
            fetched_at: DateTime::<Utc>::UNIX_EPOCH,
            data: Task {
                text: "Order lamp oil".into(),
                done: false,
                done_at: None,
                due: None,
                quad: None,
                project: None,
                tags: vec![],
            },
            raw: None,
            extra: serde_json::Map::new(),
        })
    }

    #[test]
    fn e2_briefing_assembly_cannot_be_routed_to_cloud_even_deliberately() {
        // §12 forbids a cloud dependency in the core daily loop. A promise you can switch off in a
        // settings pane is a preference wearing a promise's clothes.
        let mut p = Policy::default();
        assert_eq!(p.tier(TaskClass::BriefingAssembly), Tier::Local);
        assert_eq!(
            p.set(TaskClass::BriefingAssembly, Tier::Cloud),
            Err(PolicyError::PinnedLocal(TaskClass::BriefingAssembly)),
            "the table is configuration; E-2 is not"
        );
        assert_eq!(p.tier(TaskClass::BriefingAssembly), Tier::Local);

        let mut c = Consents::default();
        assert!(c.grant(TaskClass::BriefingAssembly).is_err());
        assert!(!c.granted(TaskClass::BriefingAssembly));
    }

    #[test]
    fn the_default_policy_matches_the_requirement() {
        let p = Policy::default();
        for c in [
            TaskClass::TranscriptCleanup,
            TaskClass::IntentParse,
            TaskClass::EntityExtraction,
            TaskClass::Classification,
            TaskClass::ShortDraft,
            TaskClass::BriefingAssembly,
        ] {
            assert_eq!(p.tier(c), Tier::Local, "{c:?} should be local");
        }
        for c in [
            TaskClass::Planning,
            TaskClass::Synthesis,
            TaskClass::LongContext,
            TaskClass::ToolChain,
        ] {
            assert_eq!(p.tier(c), Tier::Cloud, "{c:?} should be cloud");
        }
    }

    #[tokio::test]
    async fn e3_nothing_leaves_without_consent_even_when_the_policy_says_cloud() {
        let brain = Brain::new(local(), Some(cloud()));
        assert_eq!(brain.policy().tier(TaskClass::Planning), Tier::Cloud);

        let (tier, why) = brain.route(TaskClass::Planning);
        assert_eq!(tier, Tier::Local);
        assert!(why.contains("consent"), "and it says why: {why}");

        let a = brain.complete(&req(TaskClass::Planning)).await.unwrap();
        assert_eq!(a.tier, Tier::Local);
        assert!(a.text.starts_with("local"));
    }

    #[tokio::test]
    async fn consent_opens_the_route_and_revoking_closes_it_again() {
        let mut brain = Brain::new(local(), Some(cloud()));
        brain.consents_mut().grant(TaskClass::Planning).unwrap();
        assert_eq!(brain.route(TaskClass::Planning).0, Tier::Cloud);
        assert!(brain
            .complete(&req(TaskClass::Planning))
            .await
            .unwrap()
            .text
            .starts_with("cloud"));

        brain.consents_mut().revoke(TaskClass::Planning);
        assert_eq!(
            brain.route(TaskClass::Planning).0,
            Tier::Local,
            "a consent you cannot withdraw is not consent"
        );
    }

    #[tokio::test]
    async fn e6_a_cloud_failure_degrades_to_local_and_says_so() {
        let mut brain = Brain::new(local(), Some(broken_cloud()));
        brain.consents_mut().grant(TaskClass::Planning).unwrap();

        let a = brain.complete(&req(TaskClass::Planning)).await.unwrap();
        assert_eq!(a.tier, Tier::Local);
        let note = a.degraded.expect("E-6: degradation must be visible");
        assert!(
            note.contains("no network"),
            "and must name the cause: {note}"
        );
        // A silently-local answer to a question you consented to send is a different product than
        // the one you agreed to.
    }

    #[tokio::test]
    async fn both_tiers_failing_is_an_error_rather_than_an_empty_answer() {
        let brain = Brain::new(
            Box::new(Fake {
                id: "local",
                tier: Tier::Local,
                fail: Some("no model loaded"),
            }),
            None,
        );
        let err = brain
            .complete(&req(TaskClass::IntentParse))
            .await
            .unwrap_err();
        assert!(matches!(err, BrainError::NoProvider { .. }));
        assert!(err.to_string().contains("no model loaded"));
    }

    #[tokio::test]
    async fn e8_every_answer_carries_citations_back_to_a_record() {
        let brain = Brain::new(local(), None);
        let a = brain.complete(&req(TaskClass::IntentParse)).await.unwrap();
        assert_eq!(a.citations.len(), 1);
        let c = &a.citations[0];
        assert_eq!(c.source, "vault");
        assert_eq!(c.source_ref, "notes/x.md#L4");
        assert!(
            c.source_ref.contains("#L"),
            "a claim you cannot jump to the source of is indistinguishable from an invention"
        );
    }

    #[test]
    fn citations_describe_what_the_model_was_given_not_what_it_claims() {
        // Asking a model to cite its own sources is asking it to be honest about something it
        // cannot check. Citations are computed from the context we supplied.
        let cites = citations_for(&[task_record()]);
        assert_eq!(cites[0].record_id, "vault:task:notes/x.md#L4");
        assert!(citations_for(&[]).is_empty());
    }

    #[test]
    fn every_task_class_is_enumerable_so_a_settings_screen_cannot_go_stale() {
        assert_eq!(TaskClass::all().len(), 10);
        let p = Policy::default();
        for c in TaskClass::all() {
            let _ = p.tier(*c);
        }
    }
}
