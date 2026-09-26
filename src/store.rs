use std::{error::Error, fs, io, path::Path, time::Duration};

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use crate::ingest::{self, PreparedDocument, Result};

const SCHEMA_VERSION: i64 = 2;

pub struct Store {
    connection: Connection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestReceipt {
    pub source_id: i64,
    pub content_hash: String,
    pub evidence_ids: Vec<i64>,
    pub duplicate: bool,
    pub review_proposal_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceChangeProposal {
    pub id: i64,
    pub source_id: i64,
    pub source_uri: String,
    pub previous_hash: String,
    pub current_hash: String,
    pub observed_at: i64,
    pub state: String,
    pub reviewer: Option<String>,
    pub decision: Option<String>,
    pub decision_date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub evidence_id: i64,
    pub source_uris: Vec<String>,
    pub excerpt: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceRecord {
    pub id: i64,
    pub text: String,
    pub source_uris: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssessmentInput {
    pub vendor: Option<String>,
    pub name: String,
    pub version: Option<String>,
    pub use_case: String,
    pub criteria: String,
    pub decision: Option<String>,
    pub decision_date: Option<String>,
    pub reviewer: Option<String>,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssessmentRecord {
    pub id: i64,
    pub vendor: Option<String>,
    pub name: String,
    pub version: Option<String>,
    pub use_case: String,
    pub criteria: String,
    pub decision: Option<String>,
    pub decision_date: Option<String>,
    pub reviewer: Option<String>,
    pub state: String,
    pub evidence: Vec<EvidenceRecord>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if path != Path::new(":memory:")
            && let Some(parent) = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;",
        )?;
        initialize_schema(&connection)?;
        Ok(Self { connection })
    }

    pub fn ingest(&mut self, document: PreparedDocument) -> Result<IngestReceipt> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let receipt = Self::ingest_document(&transaction, document, None)?;
        transaction.commit()?;
        Ok(receipt)
    }

    pub fn ingest_for_refresh(
        &mut self,
        job_id: i64,
        document: PreparedDocument,
    ) -> Result<Option<IngestReceipt>> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let active_uri: Option<String> = transaction
            .query_row(
                "SELECT s.uri
                 FROM refresh_jobs j JOIN sources s ON s.id = j.source_id
                 WHERE j.id = ?1 AND j.enabled = 1 AND j.state = 'running'",
                [job_id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(active_uri) = active_uri else {
            return Ok(None);
        };
        if document.uri != active_uri {
            return Err(invalid_input(
                "refreshed document URI does not match its scheduled source",
            ));
        }
        let receipt = Self::ingest_document(&transaction, document, Some(job_id))?;
        transaction.commit()?;
        Ok(Some(receipt))
    }

    fn ingest_document(
        transaction: &rusqlite::Transaction<'_>,
        document: PreparedDocument,
        refresh_job_id: Option<i64>,
    ) -> Result<IngestReceipt> {
        if document.uri.is_empty() || document.extracted_text.trim().is_empty() {
            return Err(invalid_input("source URI and extracted text are required"));
        }
        if document.raw_content.len() > ingest::MAX_SOURCE_BYTES
            || document.extracted_text.len() > ingest::MAX_SOURCE_BYTES * 4
        {
            return Err(invalid_input(
                "source content exceeds the storage size limit",
            ));
        }
        if !matches!(
            document.media_type.as_str(),
            "text/html" | "application/xhtml+xml" | "text/plain" | "text/markdown"
        ) {
            return Err(invalid_input("source media type is not supported"));
        }
        std::str::from_utf8(&document.raw_content)
            .map_err(|_| invalid_input("source is not valid UTF-8 text"))?;
        if document.content_hash != ingest::hash_bytes(&document.raw_content) {
            return Err(invalid_input(
                "source content hash does not match its bytes",
            ));
        }
        let segments = ingest::split_evidence(&document.extracted_text);
        if segments.is_empty() {
            return Err(invalid_input("source contains no evidence text"));
        }
        let existing_source_id: Option<i64> = transaction
            .query_row(
                "SELECT id FROM sources WHERE uri = ?1",
                [&document.uri],
                |row| row.get(0),
            )
            .optional()?;
        let previous_hash: Option<String> = if let Some(source_id) = existing_source_id {
            transaction
                .query_row(
                    "SELECT content_hash FROM source_observations
                     WHERE source_id = ?1 AND content_hash IS NOT NULL AND status = 'stored'
                     ORDER BY retrieved_at DESC, id DESC LIMIT 1",
                    [source_id],
                    |row| row.get(0),
                )
                .optional()?
        } else {
            None
        };
        let existed: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM documents WHERE content_hash = ?1)",
            [&document.content_hash],
            |row| row.get(0),
        )?;
        transaction.execute(
            "INSERT INTO documents (
                 content_hash, media_type, byte_len, raw_content, extracted_text,
                 extraction_status, first_retrieved_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, 'ok', ?6)
             ON CONFLICT(content_hash) DO NOTHING",
            params![
                document.content_hash,
                document.media_type,
                document.raw_content.len() as i64,
                document.raw_content,
                document.extracted_text,
                document.retrieved_at,
            ],
        )?;
        let kind = if document.uri.starts_with("https://") {
            if ingest::safe_source_uri(&document.uri).as_deref() != Some(document.uri.as_str()) {
                return Err(invalid_input(
                    "web source URI must be canonical HTTPS without embedded credentials",
                ));
            }
            "web"
        } else if let Ok(url) = reqwest::Url::parse(&document.uri) {
            if url.scheme() != "file" {
                return Err(invalid_input("source URI must use HTTPS or file"));
            }
            "local"
        } else {
            return Err(invalid_input("source URI must use HTTPS or file"));
        };
        transaction.execute(
            "INSERT INTO sources (
                 uri, kind, title, first_seen_at, last_seen_at, etag, last_modified_at
             ) VALUES (?1, ?2, ?3, ?4, ?4, ?5, ?6)
             ON CONFLICT(uri) DO UPDATE SET
                 title = COALESCE(excluded.title, sources.title),
                 last_seen_at = excluded.last_seen_at,
                 etag = excluded.etag,
                 last_modified_at = excluded.last_modified_at",
            params![
                document.uri,
                kind,
                document.title,
                document.retrieved_at,
                document.etag,
                document.last_modified_at,
            ],
        )?;
        let source_id: i64 = transaction.query_row(
            "SELECT id FROM sources WHERE uri = ?1",
            [&document.uri],
            |row| row.get(0),
        )?;
        transaction.execute(
            "INSERT INTO source_observations (
                 source_id, content_hash, retrieved_at, published_at, last_modified_at,
                 extraction_status, status
             ) VALUES (?1, ?2, ?3, ?4, ?5, 'ok', 'stored')
             ON CONFLICT(source_id, content_hash) DO UPDATE SET
                 retrieved_at = excluded.retrieved_at,
                 published_at = COALESCE(excluded.published_at, source_observations.published_at),
                 last_modified_at = COALESCE(excluded.last_modified_at, source_observations.last_modified_at),
                 status = 'stored', error = NULL",
            params![
                source_id,
                document.content_hash,
                document.retrieved_at,
                document.published_at,
                document.last_modified_at,
            ],
        )?;
        if !existed {
            for (ordinal, text) in segments.iter().enumerate() {
                transaction.execute(
                    "INSERT INTO evidence (content_hash, ordinal, text) VALUES (?1, ?2, ?3)",
                    params![document.content_hash, ordinal as i64, text],
                )?;
            }
        }
        let review_proposal_id =
            if let (Some(job_id), Some(previous_hash)) = (refresh_job_id, previous_hash) {
                if previous_hash == document.content_hash {
                    None
                } else {
                    transaction.execute(
                        "INSERT INTO source_change_proposals (
                         source_id, refresh_job_id, previous_hash, current_hash, observed_at,
                         state
                     ) VALUES (?1, ?2, ?3, ?4, ?5, 'pending')
                     ON CONFLICT DO NOTHING",
                        params![
                            source_id,
                            job_id,
                            previous_hash,
                            document.content_hash,
                            document.retrieved_at,
                        ],
                    )?;
                    transaction
                        .query_row(
                            "SELECT id FROM source_change_proposals
                         WHERE source_id = ?1 AND previous_hash = ?2 AND current_hash = ?3
                           AND state = 'pending'",
                            params![source_id, previous_hash, document.content_hash],
                            |row| row.get(0),
                        )
                        .optional()?
                }
            } else {
                None
            };
        let evidence_ids = evidence_ids_for_hash(transaction, &document.content_hash)?;
        Ok(IngestReceipt {
            source_id,
            content_hash: document.content_hash,
            evidence_ids,
            duplicate: existed,
            review_proposal_id,
        })
    }

    pub fn record_fetch_failure(&mut self, uri: &str, error: &str) -> Result<i64> {
        let now = ingest::now_unix();
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO sources (uri, kind, first_seen_at, last_seen_at)
             VALUES (?1, 'web', ?2, ?2)
             ON CONFLICT(uri) DO UPDATE SET last_seen_at = excluded.last_seen_at",
            params![uri, now],
        )?;
        let source_id: i64 =
            transaction.query_row("SELECT id FROM sources WHERE uri = ?1", [uri], |row| {
                row.get(0)
            })?;
        transaction.execute(
            "INSERT INTO source_observations (
                 source_id, retrieved_at, extraction_status, status, error
             ) VALUES (?1, ?2, 'failed', 'failed', ?3)",
            params![source_id, now, error],
        )?;
        transaction.commit()?;
        Ok(source_id)
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        let terms = query
            .split(|character: char| !character.is_alphanumeric())
            .filter(|term| !term.is_empty())
            .take(12)
            .map(str::to_lowercase)
            .collect::<Vec<_>>();
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        if query.chars().count() > 256 {
            return Err(invalid_input("search query exceeds 256 characters"));
        }
        let match_query = terms
            .iter()
            .map(|term| format!("\"{}\"*", term.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" AND ");
        let mut statement = self.connection.prepare(
            "SELECT e.id,
                    (SELECT GROUP_CONCAT(s.uri, char(10))
                     FROM source_observations o JOIN sources s ON s.id = o.source_id
                     WHERE o.content_hash = e.content_hash AND o.status = 'stored'),
                    e.text
             FROM evidence_search JOIN evidence e ON e.id = evidence_search.rowid
             WHERE evidence_search MATCH ?1
               AND EXISTS (
                   SELECT 1 FROM source_observations o
                   WHERE o.content_hash = e.content_hash AND o.status = 'stored'
               )
             ORDER BY bm25(evidence_search), e.id
             LIMIT ?2",
        )?;
        let hits = statement
            .query_map(params![match_query, limit.clamp(1, 100) as i64], |row| {
                let source_uris: Option<String> = row.get(1)?;
                let text: String = row.get(2)?;
                Ok(SearchHit {
                    evidence_id: row.get(0)?,
                    source_uris: source_uris
                        .unwrap_or_default()
                        .split('\n')
                        .map(str::to_owned)
                        .collect(),
                    excerpt: make_excerpt(&text, &terms),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(hits)
    }

    pub fn rebuild_search_index(&self) -> Result<()> {
        self.connection.execute(
            "INSERT INTO evidence_search(evidence_search) VALUES ('rebuild')",
            [],
        )?;
        Ok(())
    }

    pub fn create_assessment(
        &mut self,
        input: AssessmentInput,
        evidence_ids: &[i64],
    ) -> Result<i64> {
        validate_assessment(&input)?;
        if evidence_ids.is_empty() {
            return Err(invalid_input(
                "an assessment needs at least one evidence ID",
            ));
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id: i64 = transaction.query_row(
            "INSERT INTO assessments (
                 vendor, name, version, use_case, criteria, decision,
                 decision_date, reviewer, state, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)
             RETURNING id",
            params![
                non_empty(input.vendor),
                input.name.trim(),
                non_empty(input.version),
                input.use_case.trim(),
                input.criteria.trim(),
                non_empty(input.decision),
                non_empty(input.decision_date),
                non_empty(input.reviewer),
                input.state,
                ingest::now_unix(),
            ],
            |row| row.get(0),
        )?;
        for evidence_id in evidence_ids
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
        {
            transaction.execute(
                "INSERT INTO assessment_evidence (assessment_id, evidence_id) VALUES (?1, ?2)",
                params![id, evidence_id],
            )?;
        }
        transaction.commit()?;
        Ok(id)
    }

    pub fn review_assessment(
        &mut self,
        id: i64,
        state: &str,
        reviewer: Option<&str>,
        decision: Option<&str>,
        decision_date: Option<&str>,
    ) -> Result<()> {
        let mut assessment = self
            .assessment(id)?
            .ok_or_else(|| invalid_input(format!("assessment {id} does not exist")))?;
        assessment.state = state.trim().to_owned();
        assessment.reviewer = reviewer
            .map(|value| value.trim().to_owned())
            .or(assessment.reviewer);
        assessment.decision = decision
            .map(|value| value.trim().to_owned())
            .or(assessment.decision);
        assessment.decision_date = decision_date
            .map(|value| value.trim().to_owned())
            .or(assessment.decision_date);
        validate_assessment(&AssessmentInput {
            vendor: assessment.vendor.clone(),
            name: assessment.name.clone(),
            version: assessment.version.clone(),
            use_case: assessment.use_case.clone(),
            criteria: assessment.criteria.clone(),
            decision: assessment.decision.clone(),
            decision_date: assessment.decision_date.clone(),
            reviewer: assessment.reviewer.clone(),
            state: assessment.state.clone(),
        })?;
        self.connection.execute(
            "UPDATE assessments
             SET state = ?1, reviewer = ?2, decision = ?3, decision_date = ?4, updated_at = ?5
             WHERE id = ?6",
            params![
                assessment.state,
                assessment.reviewer,
                assessment.decision,
                assessment.decision_date,
                ingest::now_unix(),
                id,
            ],
        )?;
        Ok(())
    }

    pub fn assessments(&self, approved_only: bool) -> Result<Vec<AssessmentRecord>> {
        let filter = if approved_only {
            "WHERE state = 'approved'"
        } else {
            ""
        };
        let sql = format!(
            "SELECT id, vendor, name, version, use_case, criteria, decision, decision_date, reviewer, state
             FROM assessments {filter}
             ORDER BY lower(name), COALESCE(version, ''), id"
        );
        let mut statement = self.connection.prepare(&sql)?;
        let records = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, String>(9)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut assessments = Vec::with_capacity(records.len());
        for row in records {
            let (
                id,
                vendor,
                name,
                version,
                use_case,
                criteria,
                decision,
                decision_date,
                reviewer,
                state,
            ) = row;
            assessments.push(AssessmentRecord {
                id,
                vendor,
                name,
                version,
                use_case,
                criteria,
                decision,
                decision_date,
                reviewer,
                state,
                evidence: self.evidence_for_assessment(id)?,
            });
        }
        Ok(assessments)
    }

    pub fn assessment(&self, id: i64) -> Result<Option<AssessmentRecord>> {
        let row = self
            .connection
            .query_row(
                "SELECT id, vendor, name, version, use_case, criteria, decision, decision_date, reviewer, state
                 FROM assessments WHERE id = ?1",
                [id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, Option<String>>(6)?,
                        row.get::<_, Option<String>>(7)?,
                        row.get::<_, Option<String>>(8)?,
                        row.get::<_, String>(9)?,
                    ))
                },
            )
            .optional()?;
        row.map(
            |(
                id,
                vendor,
                name,
                version,
                use_case,
                criteria,
                decision,
                decision_date,
                reviewer,
                state,
            )| {
                Ok(AssessmentRecord {
                    id,
                    vendor,
                    name,
                    version,
                    use_case,
                    criteria,
                    decision,
                    decision_date,
                    reviewer,
                    state,
                    evidence: self.evidence_for_assessment(id)?,
                })
            },
        )
        .transpose()
    }

    pub fn set_refresh_schedule(&mut self, source_id: i64, interval_secs: i64) -> Result<i64> {
        if !(300..=31_536_000).contains(&interval_secs) {
            return Err(invalid_input(
                "refresh interval must be between 300 and 31,536,000 seconds",
            ));
        }
        let uri: Option<String> = self
            .connection
            .query_row(
                "SELECT uri FROM sources WHERE id = ?1",
                [source_id],
                |row| row.get(0),
            )
            .optional()?;
        let uri = uri.ok_or_else(|| invalid_input(format!("source {source_id} does not exist")))?;
        if !uri.starts_with("https://") {
            return Err(invalid_input(
                "only HTTPS sources can be scheduled for refresh",
            ));
        }
        let id: i64 = self.connection.query_row(
            "INSERT INTO refresh_jobs (
                 source_id, interval_secs, enabled, next_run_at, state, attempts, created_at
             ) VALUES (?1, ?2, 1, ?3, 'scheduled', 0, ?3)
             ON CONFLICT(source_id) DO UPDATE SET
                 interval_secs = excluded.interval_secs,
                 enabled = 1, lease_until = NULL,
                 next_run_at = excluded.next_run_at,
                 state = 'scheduled', attempts = 0, last_error = NULL
             RETURNING id",
            params![source_id, interval_secs, ingest::now_unix()],
            |row| row.get(0),
        )?;
        Ok(id)
    }

    pub fn refresh_jobs(&self) -> Result<Vec<RefreshJob>> {
        let mut statement = self.connection.prepare(
            "SELECT j.id, j.source_id, s.uri, j.interval_secs, j.enabled,
                    j.next_run_at, j.state, j.attempts, j.last_error
             FROM refresh_jobs j JOIN sources s ON s.id = j.source_id
             ORDER BY j.id",
        )?;
        Ok(statement
            .query_map([], |row| {
                Ok(RefreshJob {
                    id: row.get(0)?,
                    source_id: row.get(1)?,
                    uri: row.get(2)?,
                    interval_secs: row.get(3)?,
                    enabled: row.get(4)?,
                    next_run_at: row.get(5)?,
                    state: row.get(6)?,
                    attempts: row.get(7)?,
                    last_error: row.get(8)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn cancel_refresh(&mut self, job_id: i64) -> Result<()> {
        let changed = self.connection.execute(
            "UPDATE refresh_jobs SET enabled = 0, state = 'cancelled', lease_until = NULL WHERE id = ?1",
            [job_id],
        )?;
        if changed == 0 {
            return Err(invalid_input(format!(
                "refresh job {job_id} does not exist"
            )));
        }
        Ok(())
    }

    pub fn resume_refresh(&mut self, job_id: i64) -> Result<()> {
        let changed = self.connection.execute(
            "UPDATE refresh_jobs
             SET enabled = 1, state = 'scheduled', next_run_at = ?1, attempts = 0,
                 lease_until = NULL, last_error = NULL
             WHERE id = ?2",
            params![ingest::now_unix(), job_id],
        )?;
        if changed == 0 {
            return Err(invalid_input(format!(
                "refresh job {job_id} does not exist"
            )));
        }
        Ok(())
    }

    pub fn due_refresh_jobs(&self, limit: usize, now: i64) -> Result<Vec<RefreshJob>> {
        let mut statement = self.connection.prepare(
            "SELECT j.id, j.source_id, s.uri, j.interval_secs, j.enabled,
                    j.next_run_at, j.state, j.attempts, j.last_error
             FROM refresh_jobs j JOIN sources s ON s.id = j.source_id
             WHERE j.enabled = 1 AND j.next_run_at <= ?1
               AND (j.state IN ('scheduled', 'retry') OR (j.state = 'running' AND j.lease_until <= ?1))
             ORDER BY j.next_run_at, j.id LIMIT ?2",
        )?;
        Ok(statement
            .query_map(params![now, limit.clamp(1, 10) as i64], |row| {
                Ok(RefreshJob {
                    id: row.get(0)?,
                    source_id: row.get(1)?,
                    uri: row.get(2)?,
                    interval_secs: row.get(3)?,
                    enabled: row.get(4)?,
                    next_run_at: row.get(5)?,
                    state: row.get(6)?,
                    attempts: row.get(7)?,
                    last_error: row.get(8)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn start_refresh(&mut self, job: &RefreshJob, now: i64) -> Result<bool> {
        let changed = self.connection.execute(
            "UPDATE refresh_jobs
             SET state = 'running', attempts = attempts + 1, lease_until = ?1, last_error = NULL
             WHERE id = ?2 AND enabled = 1 AND next_run_at <= ?3
               AND (state IN ('scheduled', 'retry') OR (state = 'running' AND lease_until <= ?3))",
            params![now + 60, job.id, now],
        )?;
        Ok(changed == 1)
    }

    pub fn finish_refresh(
        &mut self,
        job: &RefreshJob,
        started_at: i64,
        finished_at: i64,
        outcome: &str,
        error: Option<&str>,
        next_delay: Option<i64>,
    ) -> Result<()> {
        let (state, enabled, next_run_at) = match (outcome, next_delay) {
            ("stored" | "unchanged", _) => ("scheduled", 1, finished_at + job.interval_secs),
            ("cancelled", _) => ("cancelled", 0, finished_at),
            (_, Some(delay)) => ("retry", 1, finished_at + delay),
            _ => ("failed", 1, finished_at + job.interval_secs),
        };
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "UPDATE refresh_jobs
             SET state = ?1, enabled = ?2, next_run_at = ?3, lease_until = NULL, last_error = ?4
             WHERE id = ?5 AND state = 'running'",
            params![state, enabled, next_run_at, error, job.id],
        )?;
        transaction.execute(
            "INSERT INTO refresh_attempts (job_id, started_at, finished_at, outcome, error)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![job.id, started_at, finished_at, outcome, error],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn refresh_enabled(&self, job_id: i64) -> Result<bool> {
        Ok(self.connection.query_row(
            "SELECT enabled = 1 AND state = 'running' FROM refresh_jobs WHERE id = ?1",
            [job_id],
            |row| row.get(0),
        )?)
    }

    pub fn refresh_credentials(&self, source_id: i64) -> Result<(Option<String>, Option<String>)> {
        Ok(self.connection.query_row(
            "SELECT etag, last_modified_at FROM sources WHERE id = ?1",
            [source_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?)
    }

    pub fn record_not_modified_for_refresh(
        &mut self,
        job_id: i64,
        source_id: i64,
        retrieved_at: i64,
    ) -> Result<bool> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let active: bool = transaction.query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM refresh_jobs
                 WHERE id = ?1 AND source_id = ?2 AND enabled = 1 AND state = 'running'
             )",
            params![job_id, source_id],
            |row| row.get(0),
        )?;
        if !active {
            return Ok(false);
        }
        transaction.execute(
            "UPDATE sources SET last_seen_at = ?1 WHERE id = ?2",
            params![retrieved_at, source_id],
        )?;
        transaction.execute(
            "INSERT INTO source_observations (source_id, retrieved_at, extraction_status, status)
             VALUES (?1, ?2, 'not-modified', 'unchanged')",
            params![source_id, retrieved_at],
        )?;
        transaction.commit()?;
        Ok(true)
    }

    pub fn source_change_proposals(&self, pending_only: bool) -> Result<Vec<SourceChangeProposal>> {
        let filter = if pending_only {
            "WHERE p.state = 'pending'"
        } else {
            ""
        };
        let sql = format!(
            "SELECT p.id, p.source_id, s.uri, p.previous_hash, p.current_hash,
                    p.observed_at, p.state, p.reviewer, p.decision, p.decision_date
             FROM source_change_proposals p JOIN sources s ON s.id = p.source_id
             {filter}
             ORDER BY p.observed_at, p.id"
        );
        let mut statement = self.connection.prepare(&sql)?;
        Ok(statement
            .query_map([], |row| {
                Ok(SourceChangeProposal {
                    id: row.get(0)?,
                    source_id: row.get(1)?,
                    source_uri: row.get(2)?,
                    previous_hash: row.get(3)?,
                    current_hash: row.get(4)?,
                    observed_at: row.get(5)?,
                    state: row.get(6)?,
                    reviewer: row.get(7)?,
                    decision: row.get(8)?,
                    decision_date: row.get(9)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn review_source_change(
        &mut self,
        proposal_id: i64,
        state: &str,
        reviewer: &str,
        decision: &str,
        decision_date: &str,
    ) -> Result<()> {
        if !matches!(state, "accepted" | "rejected") {
            return Err(invalid_input(
                "source change review state must be accepted or rejected",
            ));
        }
        if reviewer.trim().is_empty() || decision.trim().is_empty() {
            return Err(invalid_input(
                "source change review needs a reviewer and decision",
            ));
        }
        if !is_iso_date(decision_date) {
            return Err(invalid_input("decision date must use YYYY-MM-DD"));
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = transaction.execute(
            "UPDATE source_change_proposals
             SET state = ?1, reviewer = ?2, decision = ?3, decision_date = ?4
             WHERE id = ?5 AND state = 'pending'",
            params![
                state,
                reviewer.trim(),
                decision.trim(),
                decision_date,
                proposal_id,
            ],
        )?;
        if changed == 0 {
            return Err(invalid_input(format!(
                "pending source change proposal {proposal_id} does not exist"
            )));
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn source_uri(&self, source_id: i64) -> Result<String> {
        self.connection
            .query_row(
                "SELECT uri FROM sources WHERE id = ?1",
                [source_id],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    fn evidence_for_assessment(&self, assessment_id: i64) -> Result<Vec<EvidenceRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT e.id, e.text, GROUP_CONCAT(s.uri, char(10))
             FROM assessment_evidence ae
             JOIN evidence e ON e.id = ae.evidence_id
             JOIN source_observations o ON o.content_hash = e.content_hash AND o.status = 'stored'
             JOIN sources s ON s.id = o.source_id
             WHERE ae.assessment_id = ?1
             GROUP BY e.id ORDER BY e.id",
        )?;
        Ok(statement
            .query_map([assessment_id], |row| {
                let uris: String = row.get(2)?;
                Ok(EvidenceRecord {
                    id: row.get(0)?,
                    text: row.get(1)?,
                    source_uris: uris.split('\n').map(str::to_owned).collect(),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefreshJob {
    pub id: i64,
    pub source_id: i64,
    pub uri: String,
    pub interval_secs: i64,
    pub enabled: bool,
    pub next_run_at: i64,
    pub state: String,
    pub attempts: i64,
    pub last_error: Option<String>,
}

fn initialize_schema(connection: &Connection) -> Result<()> {
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version > SCHEMA_VERSION {
        return Err(invalid_input(format!(
            "database schema version {version} is newer than this application"
        )));
    }
    if version == SCHEMA_VERSION {
        return Ok(());
    }
    if version == 1 {
        migrate_source_change_proposals(connection)?;
        return Ok(());
    }
    connection.execute_batch(
        "BEGIN IMMEDIATE;
         CREATE TABLE IF NOT EXISTS sources (
             id INTEGER PRIMARY KEY,
             uri TEXT NOT NULL UNIQUE,
             kind TEXT NOT NULL CHECK(kind IN ('web', 'local')),
             title TEXT,
             first_seen_at INTEGER NOT NULL,
             last_seen_at INTEGER NOT NULL,
             etag TEXT,
             last_modified_at TEXT
         );
         CREATE TABLE IF NOT EXISTS documents (
             content_hash TEXT PRIMARY KEY,
             media_type TEXT NOT NULL,
             byte_len INTEGER NOT NULL CHECK(byte_len >= 0),
             raw_content BLOB NOT NULL,
             extracted_text TEXT NOT NULL,
             extraction_status TEXT NOT NULL,
             first_retrieved_at INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS source_observations (
             id INTEGER PRIMARY KEY,
             source_id INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
             content_hash TEXT REFERENCES documents(content_hash),
             retrieved_at INTEGER NOT NULL,
             published_at TEXT,
             last_modified_at TEXT,
             extraction_status TEXT NOT NULL,
             status TEXT NOT NULL,
             error TEXT,
             UNIQUE(source_id, content_hash)
         );
         CREATE INDEX IF NOT EXISTS source_observations_hash_idx
             ON source_observations(content_hash);
         CREATE TABLE IF NOT EXISTS evidence (
             id INTEGER PRIMARY KEY,
             content_hash TEXT NOT NULL REFERENCES documents(content_hash) ON DELETE CASCADE,
             ordinal INTEGER NOT NULL,
             text TEXT NOT NULL,
             UNIQUE(content_hash, ordinal)
         );
         CREATE VIRTUAL TABLE IF NOT EXISTS evidence_search USING fts5(
             text, content='evidence', content_rowid='id', prefix='2 3'
         );
         CREATE TRIGGER IF NOT EXISTS evidence_search_insert AFTER INSERT ON evidence BEGIN
             INSERT INTO evidence_search(rowid, text) VALUES (new.id, new.text);
         END;
         CREATE TRIGGER IF NOT EXISTS evidence_search_delete AFTER DELETE ON evidence BEGIN
             INSERT INTO evidence_search(evidence_search, rowid, text) VALUES ('delete', old.id, old.text);
         END;
         CREATE TRIGGER IF NOT EXISTS evidence_search_update AFTER UPDATE ON evidence BEGIN
             INSERT INTO evidence_search(evidence_search, rowid, text) VALUES ('delete', old.id, old.text);
             INSERT INTO evidence_search(rowid, text) VALUES (new.id, new.text);
         END;
         CREATE TABLE IF NOT EXISTS assessments (
             id INTEGER PRIMARY KEY,
             vendor TEXT,
             name TEXT NOT NULL,
             version TEXT,
             use_case TEXT NOT NULL,
             criteria TEXT NOT NULL,
             decision TEXT,
             decision_date TEXT,
             reviewer TEXT,
             state TEXT NOT NULL CHECK(state IN ('draft', 'approved', 'rejected')),
             created_at INTEGER NOT NULL,
             updated_at INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS assessment_evidence (
             assessment_id INTEGER NOT NULL REFERENCES assessments(id) ON DELETE CASCADE,
             evidence_id INTEGER NOT NULL REFERENCES evidence(id),
             PRIMARY KEY(assessment_id, evidence_id)
         );
         CREATE TABLE IF NOT EXISTS refresh_jobs (
             id INTEGER PRIMARY KEY,
             source_id INTEGER NOT NULL UNIQUE REFERENCES sources(id) ON DELETE CASCADE,
             interval_secs INTEGER NOT NULL CHECK(interval_secs BETWEEN 300 AND 31536000),
             enabled INTEGER NOT NULL CHECK(enabled IN (0, 1)),
             next_run_at INTEGER NOT NULL,
             state TEXT NOT NULL CHECK(state IN ('scheduled', 'running', 'retry', 'failed', 'cancelled')),
             attempts INTEGER NOT NULL DEFAULT 0,
             lease_until INTEGER,
             last_error TEXT,
             created_at INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS refresh_attempts (
             id INTEGER PRIMARY KEY,
             job_id INTEGER NOT NULL REFERENCES refresh_jobs(id) ON DELETE CASCADE,
             started_at INTEGER NOT NULL,
             finished_at INTEGER NOT NULL,
             outcome TEXT NOT NULL,
             error TEXT
         );
         CREATE INDEX IF NOT EXISTS refresh_due_idx ON refresh_jobs(enabled, next_run_at, state);
         PRAGMA user_version = 1;
         COMMIT;",
    )?;
    migrate_source_change_proposals(connection)?;
    Ok(())
}

fn migrate_source_change_proposals(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        "BEGIN IMMEDIATE;
         CREATE TABLE IF NOT EXISTS source_change_proposals (
             id INTEGER PRIMARY KEY,
             source_id INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
             refresh_job_id INTEGER REFERENCES refresh_jobs(id) ON DELETE SET NULL,
             previous_hash TEXT NOT NULL REFERENCES documents(content_hash),
             current_hash TEXT NOT NULL REFERENCES documents(content_hash),
             observed_at INTEGER NOT NULL,
             state TEXT NOT NULL CHECK(state IN ('pending', 'accepted', 'rejected')),
             reviewer TEXT,
             decision TEXT,
             decision_date TEXT
         );
         CREATE UNIQUE INDEX IF NOT EXISTS source_change_pending_pair_idx
             ON source_change_proposals(source_id, previous_hash, current_hash)
             WHERE state = 'pending';
         CREATE INDEX IF NOT EXISTS source_change_state_idx
             ON source_change_proposals(state, observed_at, id);
         PRAGMA user_version = 2;
         COMMIT;",
    )?;
    Ok(())
}

fn evidence_ids_for_hash(
    connection: &Connection,
    content_hash: &str,
) -> rusqlite::Result<Vec<i64>> {
    let mut statement = connection
        .prepare("SELECT id FROM evidence WHERE content_hash = ?1 ORDER BY ordinal, id")?;
    statement
        .query_map([content_hash], |row| row.get(0))?
        .collect()
}

fn make_excerpt(text: &str, terms: &[String]) -> String {
    let match_at = text.char_indices().find_map(|(index, _)| {
        let lower_suffix = text[index..].to_lowercase();
        terms
            .iter()
            .any(|term| lower_suffix.starts_with(term))
            .then_some(index)
    });
    let start = match_at.unwrap_or(0);
    let prefix = text[..start]
        .char_indices()
        .rev()
        .nth(80)
        .map_or(0, |(index, _)| index);
    let excerpt = text[prefix..]
        .chars()
        .take(240)
        .collect::<String>()
        .replace('\n', " ");
    excerpt.trim().to_owned()
}

fn validate_assessment(input: &AssessmentInput) -> Result<()> {
    if input.name.trim().is_empty()
        || input.use_case.trim().is_empty()
        || input.criteria.trim().is_empty()
    {
        return Err(invalid_input(
            "product name, use case, and criteria are required",
        ));
    }
    if !matches!(input.state.as_str(), "draft" | "approved" | "rejected") {
        return Err(invalid_input(
            "assessment state must be draft, approved, or rejected",
        ));
    }
    if matches!(input.state.as_str(), "approved" | "rejected")
        && (input
            .decision
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
            || input
                .reviewer
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
            || input
                .decision_date
                .as_deref()
                .is_none_or(|date| !is_iso_date(date)))
    {
        return Err(invalid_input(
            "a reviewed assessment needs a decision, reviewer, and YYYY-MM-DD decision date",
        ));
    }
    if let Some(date) = input.decision_date.as_deref()
        && !is_iso_date(date)
    {
        return Err(invalid_input("decision date must use YYYY-MM-DD"));
    }
    Ok(())
}

fn is_iso_date(date: &str) -> bool {
    let bytes = date.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
    {
        return false;
    }
    let year = date[..4].parse::<u16>().unwrap_or(0);
    let month = date[5..7].parse::<u8>().unwrap_or(0);
    let day = date[8..10].parse::<u8>().unwrap_or(0);
    let leap_year = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap_year => 29,
        2 => 28,
        _ => return false,
    };
    year > 0 && (1..=days_in_month).contains(&day)
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.and_then(|value| (!value.trim().is_empty()).then(|| value.trim().to_owned()))
}

fn invalid_input(message: impl Into<String>) -> Box<dyn Error + Send + Sync> {
    Box::new(io::Error::new(io::ErrorKind::InvalidInput, message.into()))
}
