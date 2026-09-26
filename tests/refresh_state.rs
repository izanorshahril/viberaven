use rusqlite::Connection;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use viberaven::{
    ingest::{PreparedDocument, hash_bytes, now_unix},
    store::Store,
};

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "viberaven-refresh-test-{}-{id}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn document(text: &str, retrieved_at: i64) -> PreparedDocument {
    let raw_content = text.as_bytes().to_vec();
    PreparedDocument {
        uri: "https://example.com/research".to_owned(),
        title: Some("Research".to_owned()),
        media_type: "text/plain".to_owned(),
        content_hash: hash_bytes(&raw_content),
        raw_content,
        extracted_text: text.to_owned(),
        published_at: None,
        last_modified_at: None,
        etag: None,
        retrieved_at,
    }
}

#[test]
fn refresh_lease_recovers_after_interruption_and_cancel_blocks_ingestion() {
    let temp = TempDir::new();
    let database = temp.path().join("catalogue.sqlite3");
    let mut store = Store::open(&database).unwrap();
    let now = now_unix() + 10;
    let source = store
        .ingest(document("The baseline source is stable.", now))
        .unwrap();
    let job_id = store.set_refresh_schedule(source.source_id, 300).unwrap();
    let first = store.due_refresh_jobs(10, now).unwrap().remove(0);
    assert_eq!(first.id, job_id);
    assert!(store.start_refresh(&first, now).unwrap());

    let mut competing_store = Store::open(&database).unwrap();
    assert!(!competing_store.start_refresh(&first, now).unwrap());
    let recovered = competing_store
        .due_refresh_jobs(10, now + 61)
        .unwrap()
        .remove(0);
    assert_eq!(recovered.state, "running");
    assert_eq!(recovered.attempts, 1);
    assert!(competing_store.start_refresh(&recovered, now + 61).unwrap());

    let mut controller = Store::open(&database).unwrap();
    controller.cancel_refresh(job_id).unwrap();
    let cancelled = competing_store
        .ingest_for_refresh(job_id, document("Changed source text.", now + 62))
        .unwrap();
    assert!(cancelled.is_none());
    assert!(!competing_store.refresh_enabled(job_id).unwrap());
    let state = competing_store.refresh_jobs().unwrap().remove(0);
    assert_eq!(state.state, "cancelled");
    assert_eq!(state.attempts, 2);
    assert!(
        competing_store
            .search("Changed source", 10)
            .unwrap()
            .is_empty()
    );

    controller.resume_refresh(job_id).unwrap();
    let resumed = competing_store
        .due_refresh_jobs(10, now + 62)
        .unwrap()
        .remove(0);
    assert!(competing_store.start_refresh(&resumed, now + 62).unwrap());
    let stored = competing_store
        .ingest_for_refresh(job_id, document("Changed source text.", now + 63))
        .unwrap();
    let stored = stored.unwrap();
    let proposal_id = stored.review_proposal_id.unwrap();
    let pending = competing_store.source_change_proposals(true).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].source_id, source.source_id);
    assert_eq!(pending[0].previous_hash, source.content_hash);
    assert_eq!(pending[0].current_hash, stored.content_hash);
    assert_eq!(pending[0].state, "pending");

    let database_argument = database.to_string_lossy().into_owned();
    let proposal_id_argument = proposal_id.to_string();
    let listed = Command::new(env!("CARGO_BIN_EXE_viberaven"))
        .args([
            "refresh",
            "changes",
            "list",
            "--db",
            database_argument.as_str(),
        ])
        .output()
        .unwrap();
    assert!(
        listed.status.success(),
        "{}",
        String::from_utf8_lossy(&listed.stderr)
    );
    assert!(
        String::from_utf8_lossy(&listed.stdout).contains(&format!("\"proposal_id\":{proposal_id}"))
    );

    let reviewed = Command::new(env!("CARGO_BIN_EXE_viberaven"))
        .args([
            "refresh",
            "changes",
            "review",
            proposal_id_argument.as_str(),
            "--db",
            database_argument.as_str(),
            "--state",
            "accepted",
            "--reviewer",
            "Ada",
            "--decision",
            "The source change was reviewed; catalogue identity remains manual.",
            "--decision-date",
            "2026-09-26",
        ])
        .output()
        .unwrap();
    assert!(
        reviewed.status.success(),
        "{}",
        String::from_utf8_lossy(&reviewed.stderr)
    );
    assert!(
        competing_store
            .source_change_proposals(true)
            .unwrap()
            .is_empty()
    );
    let history = competing_store.source_change_proposals(false).unwrap();
    assert_eq!(history[0].state, "accepted");
    assert_eq!(history[0].reviewer.as_deref(), Some("Ada"));
    competing_store
        .finish_refresh(&resumed, now + 62, now + 63, "stored", None, None)
        .unwrap();
    assert_eq!(
        competing_store.refresh_jobs().unwrap()[0].state,
        "scheduled"
    );
    assert_eq!(
        competing_store.search("Changed source", 10).unwrap().len(),
        1
    );

    let next_run = competing_store.refresh_jobs().unwrap()[0].next_run_at;
    let due = competing_store
        .due_refresh_jobs(10, next_run)
        .unwrap()
        .remove(0);
    assert!(competing_store.start_refresh(&due, next_run).unwrap());
    competing_store
        .finish_refresh(
            &due,
            next_run,
            next_run + 1,
            "failed",
            Some("timeout"),
            Some(60),
        )
        .unwrap();
    let retry = competing_store.refresh_jobs().unwrap().remove(0);
    assert_eq!(retry.state, "retry");
    assert_eq!(retry.last_error.as_deref(), Some("timeout"));
    assert!(
        competing_store
            .due_refresh_jobs(10, retry.next_run_at - 1)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        competing_store
            .due_refresh_jobs(10, retry.next_run_at)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn schema_version_one_migrates_to_reviewable_source_changes() {
    let temp = TempDir::new();
    let database = temp.path().join("catalogue.sqlite3");
    drop(Store::open(&database).unwrap());
    let connection = Connection::open(&database).unwrap();
    connection
        .execute_batch("DROP TABLE source_change_proposals; PRAGMA user_version = 1;")
        .unwrap();
    drop(connection);
    let store = Store::open(&database).unwrap();
    assert!(store.source_change_proposals(true).unwrap().is_empty());
}
