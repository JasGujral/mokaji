//! Which of the things that arrived actually ask something of you.
//!
//! **This is where X-10's lesson applies to mail.** `urgent` on a task is a typed predicate on
//! `due` rather than a regex over the task's words, because a task that happens to contain the
//! word "urgent" is not urgent and a task due in an hour is. The same trap is waiting here, in a
//! more tempting form: it is very easy to write a list of subject-line patterns — "action
//! required", "please review", "?" — and call the result a classifier. It would demo well and be
//! wrong constantly, because the words in a subject line are chosen by the sender, and the senders
//! most fluent in urgency are the ones with the least claim on your attention.
//!
//! So the signals here are **structural**: what the server told us about the message's state, and
//! what the address itself is. Every one of them is a fact about the envelope rather than a
//! reading of the prose.
//!
//! ## What this deliberately does not do
//!
//! It does not read the body — B-9 fetches headers only, so there is nothing to read. It does not
//! score, rank or "prioritise". It answers one question, conservatively: *is this plausibly
//! something a person sent you that you have not looked at yet?* Everything cleverer than that
//! belongs to M-4's brain, with a cited answer and a model that can be argued with — not to a
//! heuristic buried in a connector.

use mokaji_core::model::PersonRef;
use mokaji_net::imap::Envelope;

/// Local-parts that mean "this mailbox does not want a reply".
///
/// A closed list of *address* conventions rather than a guess at intent. These are the strings
/// senders use precisely to signal that no human is listening, which makes them one of the few
/// honest signals available from a header.
const NO_REPLY: [&str; 8] = [
    "noreply",
    "no-reply",
    "no_reply",
    "donotreply",
    "do-not-reply",
    "mailer-daemon",
    "postmaster",
    "bounce",
];

/// Whether this message plausibly asks something of you.
///
/// Three structural conditions, all of which must hold:
///
/// 1. **The server says you have not seen it.** `\Seen` is a fact reported by the mailbox, not an
///    inference. Reading it in another client clears it, which is exactly right — MOKaji should
///    agree with the client you actually use.
/// 2. **It is not from an unattended address.** A `noreply@` sender is definitionally not asking
///    for a reply.
/// 3. **It is not from you.** Mail you sent to yourself is a note, and the briefing already has a
///    place for notes.
///
/// Everything else — including anything that would require understanding what the message *says* —
/// is deliberately out of scope.
#[must_use]
pub fn needs_action(env: &Envelope, from: &PersonRef, self_addresses: &[String]) -> bool {
    if env.seen {
        return false;
    }
    let Some(email) = from.email.as_deref() else {
        // No address at all is a malformed header. Treating it as actionable would put every
        // broken sender at the top of the briefing.
        return false;
    };
    if is_self(email, self_addresses) {
        return false;
    }
    !is_unattended(email)
}

/// Whether an address belongs to the operator.
///
/// Compared on the whole address, and on the local-part before a `+` tag, so `keeper+lists@` is
/// still recognisably you.
#[must_use]
pub fn is_self(email: &str, self_addresses: &[String]) -> bool {
    let e = email.to_lowercase();
    if self_addresses.iter().any(|s| s == &e) {
        return true;
    }
    let untagged = untag(&e);
    self_addresses.iter().any(|s| untag(s) == untagged)
}

/// Whether an address announces that nothing is listening behind it.
#[must_use]
pub fn is_unattended(email: &str) -> bool {
    let local = email.split('@').next().unwrap_or_default().to_lowercase();
    let bare: String = local.chars().filter(char::is_ascii_alphanumeric).collect();
    NO_REPLY.iter().any(|n| {
        let n_bare: String = n.chars().filter(char::is_ascii_alphanumeric).collect();
        bare == n_bare || bare.starts_with(&n_bare)
    })
}

fn untag(email: &str) -> String {
    match email.split_once('@') {
        Some((local, domain)) => {
            let base = local.split('+').next().unwrap_or(local);
            format!("{base}@{domain}")
        }
        None => email.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(seen: bool) -> Envelope {
        Envelope {
            uid: 1,
            from: String::new(),
            subject: "Fog signal inspection window".into(),
            date: "Sat, 22 Aug 2026 09:14:00 +0100".into(),
            message_id: "abc@example.org".into(),
            seen,
        }
    }

    fn who(email: &str) -> PersonRef {
        PersonRef {
            name: None,
            email: Some(email.into()),
        }
    }

    const ME: [&str; 1] = ["keeper@example.org"];
    fn me() -> Vec<String> {
        ME.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn an_unseen_message_from_a_person_asks_something_of_you() {
        assert!(needs_action(&env(false), &who("ops@example.net"), &me()));
    }

    #[test]
    fn seen_is_a_fact_from_the_server_rather_than_an_inference() {
        // Reading it in Mail.app clears \Seen, and MOKaji should agree with the client you
        // actually use rather than keep its own opinion.
        assert!(!needs_action(&env(true), &who("ops@example.net"), &me()));
    }

    #[test]
    fn unattended_senders_are_not_asking_for_anything() {
        for a in [
            "noreply@example.net",
            "no-reply@example.net",
            "do_not_reply@example.net",
            "mailer-daemon@example.net",
            "bounces+123@example.net",
        ] {
            assert!(!needs_action(&env(false), &who(a), &me()), "{a}");
        }
        // A real person whose name merely starts with one of those letters is not caught.
        assert!(needs_action(&env(false), &who("noel@example.net"), &me()));
    }

    #[test]
    fn mail_from_yourself_is_a_note_rather_than_a_request() {
        assert!(!needs_action(
            &env(false),
            &who("keeper@example.org"),
            &me()
        ));
        // Plus-tagging is still you.
        assert!(!needs_action(
            &env(false),
            &who("keeper+lists@example.org"),
            &me()
        ));
        assert!(!needs_action(
            &env(false),
            &who("KEEPER@Example.ORG"),
            &me()
        ));
    }

    #[test]
    fn a_missing_address_is_not_actionable() {
        let anon = PersonRef {
            name: Some("Harbour Office".into()),
            email: None,
        };
        // A malformed header must not put every broken sender at the top of the briefing.
        assert!(!needs_action(&env(false), &anon, &me()));
    }

    #[test]
    fn the_subject_line_is_never_consulted() {
        // X-10's lesson, applied to mail: the words in a subject are chosen by the sender, and the
        // senders most fluent in urgency have the least claim on your attention. Two messages that
        // differ only in wording must classify identically.
        let mut shouty = env(false);
        shouty.subject = "URGENT: ACTION REQUIRED — RESPOND IMMEDIATELY".into();
        let mut calm = env(false);
        calm.subject = "notes from yesterday".into();
        let sender = who("ops@example.net");
        assert_eq!(
            needs_action(&shouty, &sender, &me()),
            needs_action(&calm, &sender, &me())
        );
    }
}
