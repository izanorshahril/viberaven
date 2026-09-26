use rusqlite::Connection;
use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};
use viberaven::{
    export::{self, ExportFormat},
    ingest::{PreparedDocument, hash_bytes, now_unix, prepare_local},
    store::{AssessmentInput, Store},
};

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("viberaven-test-{}-{id}", std::process::id()));
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

fn run(args: &[&OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_viberaven"))
        .args(args)
        .output()
        .unwrap()
}

fn args<'a>(values: &'a [&'a str]) -> Vec<&'a OsStr> {
    values.iter().map(OsStr::new).collect()
}

#[test]
fn local_evidence_is_idempotent_searchable_reviewable_and_exportable() {
    let temp = TempDir::new();
    let article = temp.path().join("article.md");
    let database = temp.path().join("catalogue.sqlite3");
    fs::write(
        &article,
        "Alpha Notebook 2.0\n\nSupports offline editing and local search.\n\nThe release notes say local indexing is optional.\n",
    )
    .unwrap();
    let article = article.to_string_lossy().into_owned();
    let database = database.to_string_lossy().into_owned();
    let root = temp.path().to_string_lossy().into_owned();

    let first = run(&args(&[
        "ingest",
        "--db",
        &database,
        "--allow-root",
        &root,
        &article,
    ]));
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first = String::from_utf8(first.stdout).unwrap();
    assert!(first.contains("\"source_id\":1"), "{first}");
    assert!(first.contains("\"evidence_ids\":[1,2,3]"), "{first}");
    assert!(first.contains("\"duplicate\":false"), "{first}");

    let duplicate = run(&args(&[
        "ingest",
        "--db",
        &database,
        "--allow-root",
        &root,
        &article,
    ]));
    assert!(
        duplicate.status.success(),
        "{}",
        String::from_utf8_lossy(&duplicate.stderr)
    );
    let duplicate = String::from_utf8(duplicate.stdout).unwrap();
    assert!(duplicate.contains("\"source_id\":1"), "{duplicate}");
    assert!(duplicate.contains("\"duplicate\":true"), "{duplicate}");

    let mirror = temp.path().join("mirror.md");
    fs::copy(&article, &mirror).unwrap();
    let mirror = mirror.to_string_lossy().into_owned();
    let mirrored = run(&args(&[
        "ingest",
        "--db",
        &database,
        "--allow-root",
        &root,
        &mirror,
    ]));
    assert!(
        mirrored.status.success(),
        "{}",
        String::from_utf8_lossy(&mirrored.stderr)
    );
    let mirrored = String::from_utf8(mirrored.stdout).unwrap();
    assert!(mirrored.contains("\"evidence_ids\":[1,2,3]"), "{mirrored}");
    assert!(mirrored.contains("\"duplicate\":true"), "{mirrored}");

    let results = run(&args(&["search", "--db", &database, "optional index"]));
    assert!(
        results.status.success(),
        "{}",
        String::from_utf8_lossy(&results.stderr)
    );
    let results = String::from_utf8(results.stdout).unwrap();
    assert!(results.contains("article.md"), "{results}");
    assert!(results.contains("local indexing is optional"), "{results}");
    assert!(results.contains("mirror.md"), "{results}");

    let partial = run(&args(&["search", "--db", &database, "option"]));
    assert!(
        partial.status.success(),
        "{}",
        String::from_utf8_lossy(&partial.stderr)
    );
    assert!(String::from_utf8_lossy(&partial.stdout).contains("optional"));
    let empty = run(&args(&["search", "--db", &database, "!!!"]));
    assert!(
        empty.status.success(),
        "{}",
        String::from_utf8_lossy(&empty.stderr)
    );
    assert!(empty.stdout.is_empty());

    let connection = Connection::open(&database).unwrap();
    connection
        .execute_batch(
            "DROP TABLE evidence_search;
             CREATE VIRTUAL TABLE evidence_search USING fts5(
                 text, content='evidence', content_rowid='id', prefix='2 3'
             );",
        )
        .unwrap();
    drop(connection);
    let rebuild = run(&args(&["rebuild-search", "--db", &database]));
    assert!(
        rebuild.status.success(),
        "{}",
        String::from_utf8_lossy(&rebuild.stderr)
    );
    let rebuilt = run(&args(&["search", "--db", &database, "optional"]));
    assert!(
        rebuilt.status.success(),
        "{}",
        String::from_utf8_lossy(&rebuilt.stderr)
    );
    assert!(String::from_utf8_lossy(&rebuilt.stdout).contains("optional"));

    let assessment = run(&args(&[
        "assessment",
        "add",
        "--db",
        &database,
        "--name",
        "Alpha Notebook",
        "--version",
        "2.0",
        "--use-case",
        "offline research",
        "--criteria",
        "offline editing=pass; local search=unknown",
        "--decision",
        "review the optional index claim",
        "--reviewer",
        "Ada",
        "--decision-date",
        "2026-09-26",
        "--state",
        "approved",
        "--evidence",
        "1,3",
    ]));
    assert!(
        assessment.status.success(),
        "{}",
        String::from_utf8_lossy(&assessment.stderr)
    );
    assert!(
        String::from_utf8_lossy(&assessment.stdout).contains("\"assessment_id\":1"),
        "{}",
        String::from_utf8_lossy(&assessment.stdout)
    );

    let preview = run(&args(&[
        "export", "preview", "--db", &database, "--format", "csv",
    ]));
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    let preview = String::from_utf8(preview.stdout).unwrap();
    assert!(
        preview.starts_with("assessment_id,vendor,product,version,use_case"),
        "{preview}"
    );
    assert!(preview.contains("offline research"), "{preview}");
    assert!(preview.contains("evidence:1"), "{preview}");

    let repeated_preview = run(&args(&[
        "export", "preview", "--db", &database, "--format", "csv",
    ]));
    assert!(repeated_preview.status.success());
    assert_eq!(preview, String::from_utf8(repeated_preview.stdout).unwrap());
}

#[test]
fn local_ingest_rejects_paths_outside_the_explicit_root() {
    let temp = TempDir::new();
    let root = temp.path().join("allowed");
    let outside = temp.path().join("outside.md");
    fs::create_dir_all(&root).unwrap();
    fs::write(&outside, "outside input\n").unwrap();
    let database = temp.path().join("catalogue.sqlite3");
    let output = run(&args(&[
        "ingest",
        "--db",
        database.to_str().unwrap(),
        "--allow-root",
        root.to_str().unwrap(),
        outside.to_str().unwrap(),
    ]));
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("outside"));
    assert!(!database.exists());
}

#[test]
fn malformed_local_input_is_rejected_before_database_creation() {
    let temp = TempDir::new();
    let root = temp.path().join("allowed");
    fs::create_dir_all(&root).unwrap();
    let malformed = root.join("malformed.md");
    fs::write(&malformed, [0xff, 0xfe, 0xfd]).unwrap();
    let database = temp.path().join("catalogue.sqlite3");
    let output = run(&args(&[
        "ingest",
        "--db",
        database.to_str().unwrap(),
        "--allow-root",
        root.to_str().unwrap(),
        malformed.to_str().unwrap(),
    ]));
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("UTF-8"));
    assert!(!database.exists());
}

#[test]
fn local_html_extraction_keeps_title_and_publication_date() {
    let temp = TempDir::new();
    let article = temp.path().join("article.html");
    fs::write(
        &article,
        "<html><head><title>Alpha &amp; Beta</title><meta property=\"article:published_time\" content=\"2026-09-20\"></head><body><p>Published product evidence.</p></body></html>",
    )
    .unwrap();
    let document = prepare_local(&article, temp.path()).unwrap();
    assert_eq!(document.title.as_deref(), Some("Alpha & Beta"));
    assert_eq!(document.published_at.as_deref(), Some("2026-09-20"));
    assert!(
        document
            .extracted_text
            .contains("Published product evidence.")
    );
}

#[test]
fn reviewed_assessments_require_a_valid_calendar_date() {
    let temp = TempDir::new();
    let source = temp.path().join("evidence.md");
    let database = temp.path().join("catalogue.sqlite3");
    fs::write(&source, "A source supports local indexing.\n").unwrap();
    let source = source.to_string_lossy().into_owned();
    let database = database.to_string_lossy().into_owned();
    let root = temp.path().to_string_lossy().into_owned();
    let ingest = run(&args(&[
        "ingest",
        "--db",
        &database,
        "--allow-root",
        &root,
        &source,
    ]));
    assert!(
        ingest.status.success(),
        "{}",
        String::from_utf8_lossy(&ingest.stderr)
    );

    let invalid = run(&args(&[
        "assessment",
        "add",
        "--db",
        &database,
        "--name",
        "Alpha",
        "--use-case",
        "research",
        "--criteria",
        "optional indexing",
        "--decision",
        "review evidence",
        "--reviewer",
        "Ada",
        "--decision-date",
        "2026-02-29",
        "--state",
        "approved",
        "--evidence",
        "1",
    ]));
    assert_eq!(invalid.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("decision date"));

    let valid = run(&args(&[
        "assessment",
        "add",
        "--db",
        &database,
        "--name",
        "Alpha",
        "--use-case",
        "research",
        "--criteria",
        "optional indexing",
        "--state",
        "draft",
        "--evidence",
        "1",
    ]));
    assert!(
        valid.status.success(),
        "{}",
        String::from_utf8_lossy(&valid.stderr)
    );
    assert!(String::from_utf8_lossy(&valid.stdout).contains("\"assessment_id\":1"));
    let reviewed = run(&args(&[
        "assessment",
        "review",
        "1",
        "--db",
        &database,
        "--state",
        "approved",
        "--decision",
        "reviewed evidence",
        "--reviewer",
        "Ada",
        "--decision-date",
        "2024-02-29",
    ]));
    assert!(
        reviewed.status.success(),
        "{}",
        String::from_utf8_lossy(&reviewed.stderr)
    );
    let listed = run(&args(&["assessment", "list", "--db", &database]));
    assert!(
        listed.status.success(),
        "{}",
        String::from_utf8_lossy(&listed.stderr)
    );
    assert!(String::from_utf8_lossy(&listed.stdout).contains("\"state\":\"approved\""));
}

#[test]
fn failed_ingest_rolls_back_before_a_retry() {
    let temp = TempDir::new();
    let database = temp.path().join("catalogue.sqlite3");
    let bytes = b"transactional evidence".to_vec();
    let hash = hash_bytes(&bytes);
    let invalid = PreparedDocument {
        uri: "http://example.com/source".to_owned(),
        title: None,
        media_type: "text/plain".to_owned(),
        content_hash: hash.clone(),
        raw_content: bytes.clone(),
        extracted_text: String::from_utf8(bytes.clone()).unwrap(),
        published_at: None,
        last_modified_at: None,
        etag: None,
        retrieved_at: now_unix(),
    };
    let valid = PreparedDocument {
        uri: "https://example.com/source".to_owned(),
        title: None,
        media_type: "text/plain".to_owned(),
        content_hash: hash,
        raw_content: bytes.clone(),
        extracted_text: String::from_utf8(bytes).unwrap(),
        published_at: None,
        last_modified_at: None,
        etag: None,
        retrieved_at: now_unix(),
    };
    let mut store = Store::open(&database).unwrap();
    assert!(store.ingest(invalid).is_err());
    let receipt = store.ingest(valid).unwrap();
    assert!(!receipt.duplicate);
    assert_eq!(receipt.evidence_ids, [1]);
}

#[test]
fn export_apply_detects_catalogue_changes_and_supports_rollback() {
    let temp = TempDir::new();
    let database = temp.path().join("catalogue.sqlite3");
    let target = temp.path().join("products.csv");
    fs::write(&target, "manually maintained content\n").unwrap();
    let mut store = Store::open(&database).unwrap();
    let receipt = store
        .ingest(PreparedDocument {
            uri: "https://example.com/alpha".to_owned(),
            title: Some("Alpha".to_owned()),
            media_type: "text/plain".to_owned(),
            content_hash: hash_bytes(b"Alpha evidence."),
            raw_content: b"Alpha evidence.".to_vec(),
            extracted_text: "Alpha evidence.".to_owned(),
            published_at: None,
            last_modified_at: None,
            etag: None,
            retrieved_at: now_unix(),
        })
        .unwrap();
    let input = |name: &str| AssessmentInput {
        vendor: Some("=formula-vendor".to_owned()),
        name: name.to_owned(),
        version: Some("1.0".to_owned()),
        use_case: "research".to_owned(),
        criteria: "contains, a comma\nand a newline".to_owned(),
        decision: Some("reviewed".to_owned()),
        decision_date: Some("2026-09-26".to_owned()),
        reviewer: Some("Ada".to_owned()),
        state: "approved".to_owned(),
    };
    store
        .create_assessment(input("Alpha"), &receipt.evidence_ids)
        .unwrap();
    let stale_preview = export::preview(&store, ExportFormat::Csv).unwrap();
    let target_before = export::target_fingerprint(&target).unwrap();
    store
        .create_assessment(input("=2+2"), &receipt.evidence_ids)
        .unwrap();
    assert!(
        export::apply(
            &store,
            ExportFormat::Csv,
            &target,
            &target_before,
            &stale_preview.sha256,
        )
        .is_err()
    );
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        "manually maintained content\n"
    );

    let preview = export::preview(&store, ExportFormat::Csv).unwrap();
    let content = String::from_utf8(preview.content.clone()).unwrap();
    assert!(content.contains("'=formula-vendor"));
    assert!(content.contains("'=2+2"));
    fs::write(&target, "concurrent edit before apply\n").unwrap();
    assert!(
        export::apply(
            &store,
            ExportFormat::Csv,
            &target,
            &target_before,
            &preview.sha256,
        )
        .is_err()
    );
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        "concurrent edit before apply\n"
    );

    let current_target = export::target_fingerprint(&target).unwrap();
    let receipt = export::apply(
        &store,
        ExportFormat::Csv,
        &target,
        &current_target,
        &preview.sha256,
    )
    .unwrap();
    assert_eq!(fs::read(&target).unwrap(), preview.content);
    export::rollback(&target, &receipt.backup, &receipt.content_sha256).unwrap();
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        "concurrent edit before apply\n"
    );

    fs::write(&target, "concurrent edit\n").unwrap();
    assert!(export::rollback(&target, &receipt.backup, &receipt.content_sha256).is_err());
    assert_eq!(fs::read_to_string(&target).unwrap(), "concurrent edit\n");

    fs::remove_file(&target).unwrap();
    let missing_target = export::target_fingerprint(&target).unwrap();
    let receipt = export::apply(
        &store,
        ExportFormat::Csv,
        &target,
        &missing_target,
        &preview.sha256,
    )
    .unwrap();
    export::rollback(&target, &receipt.backup, &receipt.content_sha256).unwrap();
    assert!(!target.exists());
}
