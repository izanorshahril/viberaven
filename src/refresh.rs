use crate::{
    ingest::{self, FetchResult, Result, fetch_url},
    store::{RefreshJob, Store},
};

const MAX_ATTEMPTS: i64 = 5;
const BASE_RETRY_SECONDS: i64 = 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefreshOutcome {
    pub job_id: i64,
    pub outcome: String,
    pub detail: String,
}

pub fn run_due(store: &mut Store, limit: usize) -> Result<Vec<RefreshOutcome>> {
    let jobs = store.due_refresh_jobs(limit, ingest::now_unix())?;
    let mut outcomes = Vec::with_capacity(jobs.len());
    for job in jobs {
        let started_at = ingest::now_unix();
        if !store.start_refresh(&job, started_at)? {
            continue;
        }
        let result = refresh_one(store, &job);
        let finished_at = ingest::now_unix();
        match result {
            Ok((outcome, detail)) => {
                let cancelled = !store.refresh_enabled(job.id)?;
                let outcome = if cancelled { "cancelled" } else { outcome };
                store.finish_refresh(&job, started_at, finished_at, outcome, None, None)?;
                outcomes.push(RefreshOutcome {
                    job_id: job.id,
                    outcome: outcome.to_owned(),
                    detail,
                });
            }
            Err(error) => {
                let message = error.to_string();
                let cancelled = !store.refresh_enabled(job.id)?;
                if !cancelled {
                    let _ = store.record_fetch_failure(&job.uri, &message);
                }
                let attempts = job.attempts + 1;
                let delay = (attempts < MAX_ATTEMPTS && !cancelled).then(|| retry_delay(attempts));
                let outcome = if cancelled { "cancelled" } else { "failed" };
                store.finish_refresh(
                    &job,
                    started_at,
                    finished_at,
                    outcome,
                    Some(&message),
                    delay,
                )?;
                outcomes.push(RefreshOutcome {
                    job_id: job.id,
                    outcome: outcome.to_owned(),
                    detail: message,
                });
            }
        }
    }
    Ok(outcomes)
}

fn refresh_one(store: &mut Store, job: &RefreshJob) -> Result<(&'static str, String)> {
    if !store.refresh_enabled(job.id)? {
        return Ok(("cancelled", "cancelled before retrieval".to_owned()));
    }
    let (etag, last_modified) = store.refresh_credentials(job.source_id)?;
    match fetch_url(&job.uri, etag.as_deref(), last_modified.as_deref())? {
        FetchResult::Modified(document) => match store.ingest_for_refresh(job.id, document)? {
            Some(receipt) => Ok((
                "stored",
                format!(
                    "content_hash={} review_proposal_id={}",
                    receipt.content_hash,
                    receipt
                        .review_proposal_id
                        .map_or_else(|| "none".to_owned(), |id| id.to_string())
                ),
            )),
            None => Ok((
                "cancelled",
                "cancelled before storing refreshed content".to_owned(),
            )),
        },
        FetchResult::NotModified { retrieved_at, .. } => {
            if store.record_not_modified_for_refresh(job.id, job.source_id, retrieved_at)? {
                Ok(("unchanged", "http_304=true".to_owned()))
            } else {
                Ok((
                    "cancelled",
                    "cancelled before recording unchanged content".to_owned(),
                ))
            }
        }
    }
}

fn retry_delay(attempts: i64) -> i64 {
    BASE_RETRY_SECONDS
        .saturating_mul(
            1_i64
                .checked_shl(attempts.saturating_sub(1) as u32)
                .unwrap_or(i64::MAX),
        )
        .min(3_600)
}
