use std::{
    thread,
    time::{Duration, Instant},
};

use anyhow::Result;
use rusqlite::{OptionalExtension, params};
use serde::Serialize;

use crate::review::{BatchItem, Problem, Store, conflict, new_id, now, validate_connector_text};

const LEASE_MS: i64 = 5 * 60 * 1000;
pub const PROTOCOL_VERSION: u8 = 1;

pub struct LocalInbox(pub Store);

#[derive(Debug, Serialize)]
pub struct Envelope {
    pub protocol_version: u8,
    pub review_id: String,
    pub session_ended: bool,
    pub next_step: &'static str,
    #[serde(flatten)]
    pub outcome: Outcome,
}

#[derive(Debug, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Outcome {
    Batch {
        batch_id: String,
        lease: String,
        expires_at_ms: i64,
        attempt: i64,
        items: Vec<BatchItem>,
    },
    Idle,
    Ended,
}

impl Envelope {
    fn new(session: &str, ended: bool, outcome: Outcome) -> Self {
        let next_step = match (&outcome, ended) {
            (Outcome::Batch { .. }, true) => {
                "Acknowledge and handle these items, then continue in the original agent chat. Do not reopen the browser review."
            }
            (Outcome::Batch { .. }, false) => {
                "Acknowledge and handle these items. Continue discussion in your agent chat."
            }
            (Outcome::Ended, _) => {
                "The browser review has ended. Stop listening and continue in the original agent chat."
            }
            (Outcome::Idle, _) => {
                "No work available before the wait expired. Poll again if you are waiting for this review."
            }
        };
        Self {
            protocol_version: PROTOCOL_VERSION,
            review_id: session.to_owned(),
            session_ended: ended,
            next_step,
            outcome,
        }
    }
}

impl LocalInbox {
    pub fn poll_connector(&self, connection: &str, wait: Duration) -> Result<Envelope> {
        let session = self.0.connector_review(connection)?;
        self.wait_for(&session, Some(connection), wait)
    }

    pub fn delivered(
        &self,
        connection: &str,
        batch_id: &str,
        token: &str,
        receipt: &str,
    ) -> Result<()> {
        validate_connector_text("Delivery receipt", receipt, 1024)?;
        self.0.write(|tx| {
            let recorded = tx
                .query_row(
                    "SELECT connector_receipt FROM batches WHERE id = ?1 AND connector_id = ?2",
                    params![batch_id, connection],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()?
                .ok_or(Problem::Missing("Batch does not belong to this connector."))?;
            if let Some(recorded) = recorded {
                return if recorded == receipt {
                    Ok(())
                } else {
                    Err(conflict("This batch already has a different receipt."))
                };
            }
            let count = tx.execute(
                "UPDATE batches SET connector_receipt = ?4, retry_at = NULL
                 WHERE id = ?1 AND connector_id = ?2 AND lease_token = ?3 AND lease_until > ?5",
                params![batch_id, connection, token, receipt, now()],
            )?;
            check_connector_lease(count)
        })
    }

    pub fn defer(
        &self,
        connection: &str,
        batch_id: &str,
        token: &str,
        delay_seconds: u32,
    ) -> Result<()> {
        self.0.write(|tx| {
            let time = now();
            let count = tx.execute(
                "UPDATE batches SET lease_token = NULL, lease_until = NULL, retry_at = ?5
                 WHERE id = ?1 AND connector_id = ?2 AND lease_token = ?3 AND lease_until > ?4
                 AND connector_receipt IS NULL",
                params![
                    batch_id,
                    connection,
                    token,
                    time,
                    time.saturating_add(i64::from(delay_seconds) * 1000)
                ],
            )?;
            check_connector_lease(count)
        })
    }

    fn wait_for(
        &self,
        session: &str,
        connection: Option<&str>,
        wait: Duration,
    ) -> Result<Envelope> {
        let started = Instant::now();
        loop {
            let result = self.claim_for(session, connection, now)?;
            if !matches!(result.outcome, Outcome::Idle) || started.elapsed() >= wait {
                return Ok(result);
            }
            thread::sleep(Duration::from_millis(200).min(wait.saturating_sub(started.elapsed())));
        }
    }

    #[cfg(test)]
    fn claim_at(&self, session_id: &str, clock: impl FnOnce() -> i64) -> Result<Envelope> {
        self.claim_for(session_id, None, clock)
    }

    fn claim_for(
        &self,
        session_id: &str,
        connection: Option<&str>,
        clock: impl FnOnce() -> i64,
    ) -> Result<Envelope> {
        self.0.write(|tx| {
            let time = clock();
            let ended: bool = tx.query_row("SELECT ended_at IS NOT NULL FROM sessions WHERE id = ?1", [session_id], |row| row.get(0))
                .optional()?.ok_or(Problem::Missing("Review not found."))?;
            let batch = tx.query_row(
                "SELECT b.id, b.attempt,
                    (b.lease_until IS NULL OR b.lease_until <= ?2) AND (b.retry_at IS NULL OR b.retry_at <= ?2)
                    FROM batches b WHERE b.session_id = ?1 AND b.connector_id IS ?3 AND b.connector_receipt IS NULL
                    AND (?3 IS NOT NULL OR b.lease_until IS NULL OR b.lease_until <= ?2)
                    AND EXISTS (SELECT 1 FROM batch_items i WHERE i.batch_id = b.id AND i.completed_at IS NULL)
                    ORDER BY b.created, b.rowid LIMIT 1",
                params![session_id, time, connection], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, bool>(2)?))
            ).optional()?;
            let outcome = if let Some((batch_id, attempt, true)) = batch {
                let lease = new_id();
                let expires_at_ms = time.saturating_add(LEASE_MS);
                tx.execute("UPDATE batches SET lease_token = ?2, lease_until = ?3, lease_received_at = NULL, attempt = attempt + 1 WHERE id = ?1",
                    params![batch_id, lease, expires_at_ms])?;
                let mut stmt = tx.prepare("SELECT snapshot FROM batch_items WHERE batch_id = ?1 AND completed_at IS NULL ORDER BY ordinal")?;
                let raw = stmt.query_map([&batch_id], |row| row.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
                let items = raw.iter().map(|snapshot| serde_json::from_str(snapshot)).collect::<serde_json::Result<_>>()?;
                Outcome::Batch { batch_id, lease, expires_at_ms, attempt: attempt + 1, items }
            } else if ended && batch.is_none() && !tx.query_row(
                "SELECT EXISTS (SELECT 1 FROM batches b JOIN batch_items i ON i.batch_id = b.id
                   WHERE b.session_id = ?1 AND b.connector_id IS ?2
                   AND b.connector_receipt IS NULL AND i.completed_at IS NULL)",
                params![session_id, connection], |row| row.get::<_, bool>(0)
            )? {
                Outcome::Ended
            } else {
                Outcome::Idle
            };
            Ok(Envelope::new(session_id, ended, outcome))
        })
    }

    #[cfg(test)]
    fn ack_at(&self, batch_id: &str, token: &str, clock: impl FnOnce() -> i64) -> Result<i64> {
        self.0.write(|tx| {
            let time = clock();
            let expires = time.saturating_add(LEASE_MS);
            let count = tx.execute(
                "UPDATE batches SET lease_until = ?4, lease_received_at = COALESCE(lease_received_at, ?3),
                   first_received_at = COALESCE(first_received_at, ?3)
                   WHERE id = ?1 AND connector_id IS NULL AND lease_token = ?2 AND lease_until > ?3",
                params![batch_id, token, time, expires],
            )?;
            if count != 1 {
                return Err(conflict("The lease is invalid or expired. Poll the review again."));
            }
            Ok(expires)
        })
    }

    #[cfg(test)]
    fn complete_at(
        &self,
        batch_id: &str,
        comment_id: &str,
        token: &str,
        clock: impl FnOnce() -> i64,
    ) -> Result<()> {
        self.0.write(|tx| {
            let time = clock();
            let completed: Option<String> = tx.query_row(
                "SELECT completed_token FROM batch_items WHERE batch_id = ?1 AND comment_id = ?2",
                params![batch_id, comment_id], |row| row.get(0),
            ).optional()?.ok_or(Problem::Missing("Item not found in this batch."))?;
            if completed.as_deref() == Some(token) {
                return Ok(());
            }
            if completed.is_some() {
                return Err(conflict("This item was completed by a different delivery attempt."));
            }
            let count = tx.execute(
                "UPDATE batch_items SET completed_at = ?4, completed_token = ?3 WHERE batch_id = ?1 AND comment_id = ?2
                 AND EXISTS (SELECT 1 FROM batches WHERE id = ?1 AND connector_id IS NULL
                    AND lease_token = ?3 AND lease_until > ?4 AND lease_received_at IS NOT NULL)",
                params![batch_id, comment_id, token, time],
            )?;
            if count != 1 {
                return Err(conflict("Acknowledge a valid lease before completing this item. Poll again if the lease expired."));
            }
            Ok(())
        })
    }
}

fn check_connector_lease(changes: usize) -> Result<()> {
    if changes != 1 {
        return Err(conflict(
            "The connector lease is invalid or expired. Poll again.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Barrier;

    use uuid::Uuid;

    use super::*;
    use crate::{
        anchor::{Anchor, Target},
        markdown::render,
        review::{DraftRef, Submission, SubmittedSource},
    };

    #[test]
    fn rolls_back_failed_send_and_end_then_recovers_after_partial_delivery() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        let store = Store::open(&root, true).unwrap();
        let review = store.current_session().unwrap();
        let paths = ["one.md", "two.md"];
        let ids = store.register(paths).unwrap();
        let doc = render("# Heading\n\nSource text.", |_| None);
        let mut items = Vec::new();
        for (number, path) in paths.iter().enumerate() {
            let body = format!("Feedback {number}");
            let anchor = Anchor::capture(
                path.to_string(),
                "rev".into(),
                &doc,
                Target::Section {
                    heading: Some("heading".into()),
                },
            )
            .unwrap();
            let id = store
                .create_comment(&review.id, &ids[*path], &body, &anchor)
                .unwrap();
            items.push(BatchItem {
                comment_id: id,
                document_id: ids[*path].clone(),
                body,
                original: anchor,
                source: SubmittedSource {
                    path: Some(path.to_string()),
                    revision: Some("rev".into()),
                    line: None,
                    anchor_status: "attached".into(),
                    source_range: None,
                },
            });
        }
        let mut request = Submission {
            key: Uuid::new_v4(),
            drafts: items
                .iter()
                .map(|i| DraftRef {
                    id: i.comment_id.clone(),
                    version: 1,
                })
                .collect(),
            end: true,
            confirmation: None,
            general_body: None,
        };
        request.drafts[1].version = 10;
        assert!(store.submit(&review.id, &request, &items).is_err());
        assert!(!store.session(&review.id).unwrap().ended);
        assert!(
            store
                .comments()
                .unwrap()
                .iter()
                .all(|c| c.batch_id.is_none())
        );
        assert!(store.submitted(&review.id, &request).unwrap().is_none());

        request.drafts[1].version = 1;
        let expected = store.submit(&review.id, &request, &items).unwrap();
        assert_eq!(
            store.submit(&review.id, &request, &items).unwrap(),
            expected
        );
        assert!(store.session(&review.id).unwrap().ended);
        let clock = now();
        let inbox = LocalInbox(store);
        let barrier = Barrier::new(2);
        let (first, second) = thread::scope(|scope| {
            let first = scope.spawn(|| {
                barrier.wait();
                inbox.claim_at(&review.id, || clock).unwrap()
            });
            barrier.wait();
            let second = inbox.claim_at(&review.id, || clock).unwrap();
            (first.join().unwrap(), second)
        });
        let (batch, idle) = if matches!(first.outcome, Outcome::Batch { .. }) {
            (first, second)
        } else {
            (second, first)
        };
        assert!(matches!(idle.outcome, Outcome::Idle));
        let Outcome::Batch {
            batch_id,
            lease,
            expires_at_ms,
            attempt: 1,
            ..
        } = batch.outcome
        else {
            panic!("expected a leased batch")
        };
        assert_eq!(batch_id, expected.batch_id);
        assert_eq!(
            inbox.0.comment(&items[0].comment_id).unwrap().delivery,
            Some("queued")
        );
        let renewed = inbox.ack_at(&batch_id, &lease, || clock + 1000).unwrap();
        assert!(renewed > expires_at_ms);
        assert_eq!(
            inbox.0.comment(&items[1].comment_id).unwrap().delivery,
            Some("received")
        );
        let one = &items[0].comment_id;
        let two = &items[1].comment_id;
        inbox
            .complete_at(&batch_id, one, &lease, || clock + 1500)
            .unwrap();
        assert!(matches!(
            inbox
                .claim_at(&review.id, || expires_at_ms)
                .unwrap()
                .outcome,
            Outcome::Idle
        ));
        assert!(
            inbox
                .complete_at(&batch_id, two, &lease, || renewed)
                .is_err()
        );
        let recovered = LocalInbox(Store::open(&root, false).unwrap());
        let next = recovered.claim_at(&review.id, || renewed).unwrap();
        let Outcome::Batch {
            items: pending,
            lease: next_token,
            attempt: 2,
            ..
        } = next.outcome
        else {
            panic!("expected interrupted work")
        };
        assert_eq!(pending.len(), 1);
        assert_eq!(&pending[0].comment_id, two);
        assert_eq!(recovered.0.comment(two).unwrap().delivery, Some("retry"));
        assert!(recovered.ack_at(&batch_id, &lease, || renewed).is_err());
        assert!(
            recovered
                .complete_at(&batch_id, two, &next_token, || renewed)
                .is_err()
        );
        recovered
            .ack_at(&batch_id, &next_token, || renewed)
            .unwrap();
        recovered
            .complete_at(&batch_id, two, &next_token, || renewed)
            .unwrap();
        recovered
            .complete_at(&batch_id, one, &lease, || renewed)
            .unwrap();
        assert!(matches!(
            recovered.claim_at(&review.id, || renewed).unwrap().outcome,
            Outcome::Ended
        ));
        assert!(!recovered.0.comment(two).unwrap().resolved);
    }
}
