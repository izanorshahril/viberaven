use std::{
    error::Error,
    ffi::OsString,
    io,
    path::{Path, PathBuf},
};

use crate::{
    VERSION,
    export::{self, ExportFormat},
    ingest::{FetchResult, Result, fetch_url, prepare_local},
    refresh,
    store::{AssessmentInput, SearchHit, Store},
};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
}

pub fn execute(input: &[OsString]) -> Result<CommandOutput> {
    let Some(command) = input.first().and_then(|value| value.to_str()) else {
        return Ok(CommandOutput {
            stdout: help_text(),
            stderr: String::new(),
        });
    };
    if matches!(command, "-h" | "--help" | "help") {
        return Ok(CommandOutput {
            stdout: help_text(),
            stderr: String::new(),
        });
    }
    if matches!(command, "-V" | "--version" | "version") {
        return Ok(CommandOutput {
            stdout: format!("viberaven {VERSION}\n"),
            stderr: String::new(),
        });
    }
    let mut args = input[1..].to_vec();
    match command {
        "ingest" => ingest_command(&mut args),
        "search" => search_command(&mut args),
        "assessment" => assessment_command(&mut args),
        "export" => export_command(&mut args),
        "refresh" => refresh_command(&mut args),
        "rebuild-search" => rebuild_search_command(&mut args),
        _ => Err(invalid_input(format!("unknown command: {command}"))),
    }
}

pub fn help_text() -> String {
    format!(
        concat!(
            "Viberaven {}\n\nUsage:\n  viberaven <COMMAND> [OPTIONS]\n\nCommands:\n",
            "  ingest <PATH|HTTPS_URL>       Import a bounded local document or public HTTPS page\n",
            "  search <QUERY>                Search extracted evidence offline\n",
            "  assessment add                Record a product/version assessment\n",
            "  assessment review <ID>        Record a human review decision\n",
            "  assessment list               List assessments\n",
            "  export preview                Preview approved assessments as CSV or README\n",
            "  export apply                  Apply a reviewed preview with conflict checks\n",
            "  export rollback               Restore a backup if the destination still matches\n",
            "  refresh schedule              Schedule a bounded HTTPS source refresh\n",
            "  refresh run                   Run due refresh jobs once\n",
            "  refresh list                  List refresh schedules and state\n",
            "  refresh changes list          List source changes waiting for human review\n",
            "  refresh changes review <ID>   Record a source change decision\n",
            "  refresh cancel <ID>           Cancel a schedule\n",
            "  refresh resume <ID>           Resume a cancelled or failed schedule\n",
            "  rebuild-search                Rebuild the derived FTS5 index\n\n",
            "Options:\n  --db PATH                Database path (default: %LOCALAPPDATA%/Viberaven/catalogue.sqlite3)\n",
            "  --allow-root PATH        Permitted local-source root (default: current directory)\n",
            "  --limit N                Search limit (1-100) or refresh job limit (1-10)\n",
            "  --format csv|readme       Export format\n",
            "  --target PATH            Explicit export destination; preview never writes it\n\n",
            "Local documents are limited to 5 MiB UTF-8 .txt, .md, .rst, and .html files.\n",
            "Web input uses HTTPS only, rejects redirects and non-public DNS results, and is limited to 5 MiB.\n",
            "Assessment state is draft, approved, or rejected; only approved assessments appear in exports.\n"
        ),
        VERSION
    )
}

fn ingest_command(args: &mut Vec<OsString>) -> Result<CommandOutput> {
    let database = take_database(args)?;
    let allow_root = take_path_option(args, "--allow-root")?
        .map(Ok)
        .unwrap_or_else(std::env::current_dir)?;
    let source = take_positionals(args)?;
    if source.len() != 1 {
        return Err(invalid_input(
            "ingest expects exactly one path or HTTPS URL",
        ));
    }
    let source_text = source[0].to_str();
    let document = if let Some(source_text) = source_text.filter(|value| value.contains("://")) {
        let mut store = Store::open(&database)?;
        let result = fetch_url(source_text, None, None);
        let document = match result {
            Ok(FetchResult::Modified(document)) => document,
            Ok(FetchResult::NotModified { .. }) => {
                return Err(invalid_input(
                    "unexpected not-modified response for a new source",
                ));
            }
            Err(error) => {
                let canonical = canonical_failure_uri(source_text);
                if let Some(uri) = canonical {
                    let _ = store.record_fetch_failure(&uri, &error.to_string());
                }
                return Err(error);
            }
        };
        let receipt = store.ingest(document)?;
        return Ok(json_line(format_ingest_receipt(&receipt)));
    } else {
        let root = allow_root.canonicalize()?;
        prepare_local(Path::new(&source[0]), &root)?
    };
    let mut store = Store::open(&database)?;
    let receipt = store.ingest(document)?;
    Ok(json_line(format_ingest_receipt(&receipt)))
}

fn search_command(args: &mut Vec<OsString>) -> Result<CommandOutput> {
    let database = take_database(args)?;
    let limit = take_text_option(args, "--limit")?
        .map(|value| parse_bounded(&value, 1, 100, "search limit"))
        .transpose()?
        .unwrap_or(20);
    let query = take_positionals(args)?
        .into_iter()
        .map(|value| {
            value
                .into_string()
                .map_err(|_| invalid_input("search query must be valid Unicode"))
        })
        .collect::<Result<Vec<_>>>()?
        .join(" ");
    let store = Store::open(&database)?;
    let hits = store.search(&query, limit)?;
    let stdout = hits
        .iter()
        .map(search_hit_json)
        .collect::<Vec<_>>()
        .join("\n");
    Ok(json_line(if stdout.is_empty() {
        String::new()
    } else {
        format!("{stdout}\n")
    }))
}

fn assessment_command(args: &mut Vec<OsString>) -> Result<CommandOutput> {
    let action = take_required_text(args, "assessment action")?;
    let database = take_database(args)?;
    let mut store = Store::open(&database)?;
    match action.as_str() {
        "add" => {
            let input = AssessmentInput {
                vendor: take_text_option(args, "--vendor")?,
                name: take_required_option(args, "--name")?,
                version: take_text_option(args, "--version")?,
                use_case: take_required_option(args, "--use-case")?,
                criteria: take_required_option(args, "--criteria")?,
                decision: take_text_option(args, "--decision")?,
                decision_date: take_text_option(args, "--decision-date")?,
                reviewer: take_text_option(args, "--reviewer")?,
                state: take_text_option(args, "--state")?.unwrap_or_else(|| "draft".to_owned()),
            };
            let evidence = parse_ids(&take_required_option(args, "--evidence")?)?;
            ensure_no_arguments(args)?;
            let id = store.create_assessment(input, &evidence)?;
            Ok(json_line(format!("{{\"assessment_id\":{id}}}")))
        }
        "review" => {
            let id = parse_id(&take_required_text(args, "assessment ID")?, "assessment ID")?;
            let state = take_required_option(args, "--state")?;
            let reviewer = take_text_option(args, "--reviewer")?;
            let decision = take_text_option(args, "--decision")?;
            let decision_date = take_text_option(args, "--decision-date")?;
            ensure_no_arguments(args)?;
            store.review_assessment(
                id,
                &state,
                reviewer.as_deref(),
                decision.as_deref(),
                decision_date.as_deref(),
            )?;
            Ok(json_line(format!(
                "{{\"assessment_id\":{id},\"state\":{}}}",
                json_string(&state)
            )))
        }
        "list" => {
            ensure_no_arguments(args)?;
            let stdout = store
                .assessments(false)?
                .iter()
                .map(assessment_json)
                .collect::<Vec<_>>()
                .join("\n");
            Ok(json_line(if stdout.is_empty() {
                String::new()
            } else {
                format!("{stdout}\n")
            }))
        }
        _ => Err(invalid_input(
            "assessment action must be add, review, or list",
        )),
    }
}

fn export_command(args: &mut Vec<OsString>) -> Result<CommandOutput> {
    let action = take_required_text(args, "export action")?;
    let database = take_database(args)?;
    let format = ExportFormat::parse(
        &take_text_option(args, "--format")?.unwrap_or_else(|| "readme".to_owned()),
    )?;
    let store = Store::open(&database)?;
    match action.as_str() {
        "preview" => {
            let target = take_path_option(args, "--target")?;
            ensure_no_arguments(args)?;
            let preview = export::preview(&store, format)?;
            let mut stderr = format!("preview_sha256={}\n", preview.sha256);
            if let Some(target) = target {
                stderr.push_str(&format!(
                    "target_sha256={}\n",
                    export::target_fingerprint(&target)?
                ));
            }
            Ok(CommandOutput {
                stdout: String::from_utf8(preview.content)
                    .map_err(|_| invalid_input("export preview is not UTF-8"))?,
                stderr,
            })
        }
        "apply" => {
            let target = take_required_path(args, "--target")?;
            let expected_target = take_required_text_option(args, "--expected-target")?;
            let expected_preview = take_required_text_option(args, "--expected-preview")?;
            ensure_no_arguments(args)?;
            let receipt =
                export::apply(&store, format, &target, &expected_target, &expected_preview)?;
            Ok(json_line(format!(
                "{{\"target\":{},\"backup\":{},\"content_sha256\":{}}}",
                json_string(&receipt.target.to_string_lossy()),
                json_string(&receipt.backup.to_string_lossy()),
                json_string(&receipt.content_sha256),
            )))
        }
        "rollback" => {
            let target = take_required_path(args, "--target")?;
            let backup = take_required_path(args, "--backup")?;
            let expected_current = take_required_text_option(args, "--expected-current")?;
            ensure_no_arguments(args)?;
            let safety_backup = export::rollback(&target, &backup, &expected_current)?;
            Ok(json_line(format!(
                "{{\"target\":{},\"safety_backup\":{}}}",
                json_string(&target.to_string_lossy()),
                json_string(&safety_backup.to_string_lossy()),
            )))
        }
        _ => Err(invalid_input(
            "export action must be preview, apply, or rollback",
        )),
    }
}

fn refresh_command(args: &mut Vec<OsString>) -> Result<CommandOutput> {
    let action = take_required_text(args, "refresh action")?;
    let database = take_database(args)?;
    let mut store = Store::open(&database)?;
    match action.as_str() {
        "schedule" => {
            let source_id = parse_id(
                &take_required_text_option(args, "--source-id")?,
                "source ID",
            )?;
            let interval = parse_bounded(
                &take_required_text_option(args, "--every-seconds")?,
                300,
                31_536_000,
                "refresh interval",
            )? as i64;
            ensure_no_arguments(args)?;
            let id = store.set_refresh_schedule(source_id, interval)?;
            Ok(json_line(format!(
                "{{\"job_id\":{id},\"state\":\"scheduled\"}}"
            )))
        }
        "list" => {
            ensure_no_arguments(args)?;
            let stdout = store
                .refresh_jobs()?
                .iter()
                .map(|job| {
                    format!(
                        "{{\"job_id\":{},\"source_id\":{},\"uri\":{},\"interval_secs\":{},\"enabled\":{},\"next_run_at\":{},\"state\":{},\"attempts\":{},\"last_error\":{}}}",
                        job.id,
                        job.source_id,
                        json_string(&job.uri),
                        job.interval_secs,
                        job.enabled,
                        job.next_run_at,
                        json_string(&job.state),
                        job.attempts,
                        json_option(job.last_error.as_deref()),
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            Ok(json_line(if stdout.is_empty() {
                String::new()
            } else {
                format!("{stdout}\n")
            }))
        }
        "run" => {
            let limit = take_text_option(args, "--limit")?
                .map(|value| parse_bounded(&value, 1, 10, "refresh job limit"))
                .transpose()?
                .unwrap_or(3);
            ensure_no_arguments(args)?;
            let output = refresh::run_due(&mut store, limit)?
                .iter()
                .map(|outcome| {
                    format!(
                        "{{\"job_id\":{},\"outcome\":{},\"detail\":{}}}",
                        outcome.job_id,
                        json_string(&outcome.outcome),
                        json_string(&outcome.detail),
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            Ok(json_line(if output.is_empty() {
                String::new()
            } else {
                format!("{output}\n")
            }))
        }
        "changes" => refresh_changes_command(args, &mut store),
        "cancel" | "resume" => {
            let id = parse_id(
                &take_required_text(args, "refresh job ID")?,
                "refresh job ID",
            )?;
            ensure_no_arguments(args)?;
            if action == "cancel" {
                store.cancel_refresh(id)?;
            } else {
                store.resume_refresh(id)?;
            }
            Ok(json_line(format!(
                "{{\"job_id\":{id},\"state\":{}}}",
                json_string(if action == "cancel" {
                    "cancelled"
                } else {
                    "scheduled"
                })
            )))
        }
        _ => Err(invalid_input(
            "refresh action must be schedule, list, run, cancel, or resume",
        )),
    }
}

fn refresh_changes_command(args: &mut Vec<OsString>, store: &mut Store) -> Result<CommandOutput> {
    let action = take_required_text(args, "source change action")?;
    match action.as_str() {
        "list" => {
            ensure_no_arguments(args)?;
            let output = store
                .source_change_proposals(true)?
                .iter()
                .map(|proposal| {
                    format!(
                        "{{\"proposal_id\":{},\"source_id\":{},\"source_uri\":{},\"previous_hash\":{},\"current_hash\":{},\"observed_at\":{},\"state\":{}}}",
                        proposal.id,
                        proposal.source_id,
                        json_string(&proposal.source_uri),
                        json_string(&proposal.previous_hash),
                        json_string(&proposal.current_hash),
                        proposal.observed_at,
                        json_string(&proposal.state),
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            Ok(json_line(if output.is_empty() {
                String::new()
            } else {
                format!("{output}\n")
            }))
        }
        "review" => {
            let id = parse_id(&take_required_text(args, "proposal ID")?, "proposal ID")?;
            let state = take_required_option(args, "--state")?;
            let reviewer = take_required_option(args, "--reviewer")?;
            let decision = take_required_option(args, "--decision")?;
            let decision_date = take_required_option(args, "--decision-date")?;
            ensure_no_arguments(args)?;
            store.review_source_change(id, &state, &reviewer, &decision, &decision_date)?;
            Ok(json_line(format!(
                "{{\"proposal_id\":{id},\"state\":{}}}\n",
                json_string(&state)
            )))
        }
        _ => Err(invalid_input(
            "refresh changes action must be list or review",
        )),
    }
}

fn rebuild_search_command(args: &mut Vec<OsString>) -> Result<CommandOutput> {
    let database = take_database(args)?;
    ensure_no_arguments(args)?;
    Store::open(&database)?.rebuild_search_index()?;
    Ok(json_line("{\"search_index\":\"rebuilt\"}\n".to_owned()))
}

fn take_database(args: &mut Vec<OsString>) -> Result<PathBuf> {
    Ok(take_path_option(args, "--db")?.unwrap_or_else(default_database_path))
}

fn default_database_path() -> PathBuf {
    let root = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    root.join("Viberaven").join("catalogue.sqlite3")
}

fn take_option(args: &mut Vec<OsString>, name: &str) -> Result<Option<OsString>> {
    let mut found = None;
    let mut index = 0;
    while index < args.len() {
        if args[index].to_str() == Some(name) {
            if found.is_some() {
                return Err(invalid_input(format!("{name} may appear only once")));
            }
            if index + 1 == args.len() {
                return Err(invalid_input(format!("{name} needs a value")));
            }
            let value = args.remove(index + 1);
            args.remove(index);
            found = Some(value);
        } else {
            index += 1;
        }
    }
    Ok(found)
}

fn take_path_option(args: &mut Vec<OsString>, name: &str) -> Result<Option<PathBuf>> {
    Ok(take_option(args, name)?.map(PathBuf::from))
}

fn take_required_path(args: &mut Vec<OsString>, name: &str) -> Result<PathBuf> {
    take_path_option(args, name)?.ok_or_else(|| invalid_input(format!("{name} is required")))
}

fn take_text_option(args: &mut Vec<OsString>, name: &str) -> Result<Option<String>> {
    take_option(args, name)?
        .map(|value| {
            value
                .into_string()
                .map_err(|_| invalid_input(format!("{name} must be valid Unicode")))
        })
        .transpose()
}

fn take_required_option(args: &mut Vec<OsString>, name: &str) -> Result<String> {
    take_text_option(args, name)?.ok_or_else(|| invalid_input(format!("{name} is required")))
}

fn take_required_text_option(args: &mut Vec<OsString>, name: &str) -> Result<String> {
    take_required_option(args, name)
}

fn take_required_text(args: &mut Vec<OsString>, label: &str) -> Result<String> {
    if args.is_empty() {
        return Err(invalid_input(format!("{label} is required")));
    }
    args.remove(0)
        .into_string()
        .map_err(|_| invalid_input(format!("{label} must be valid Unicode")))
}

fn take_positionals(args: &mut Vec<OsString>) -> Result<Vec<OsString>> {
    if let Some(argument) = args
        .iter()
        .find(|argument| argument.to_string_lossy().starts_with('-'))
    {
        return Err(invalid_input(format!(
            "unknown option: {}",
            argument.to_string_lossy()
        )));
    }
    Ok(std::mem::take(args))
}

fn ensure_no_arguments(args: &[OsString]) -> Result<()> {
    if args.is_empty() {
        Ok(())
    } else {
        Err(invalid_input(format!(
            "unexpected argument: {}",
            args[0].to_string_lossy()
        )))
    }
}

fn parse_ids(value: &str) -> Result<Vec<i64>> {
    let ids = value
        .split(',')
        .map(|part| parse_id(part.trim(), "evidence ID"))
        .collect::<Result<Vec<_>>>()?;
    if ids.is_empty() {
        return Err(invalid_input("at least one evidence ID is required"));
    }
    Ok(ids)
}

fn parse_id(value: &str, label: &str) -> Result<i64> {
    let id = value
        .parse::<i64>()
        .map_err(|_| invalid_input(format!("{label} must be a positive integer")))?;
    if id < 1 {
        return Err(invalid_input(format!("{label} must be a positive integer")));
    }
    Ok(id)
}

fn parse_bounded(value: &str, min: usize, max: usize, label: &str) -> Result<usize> {
    let number = value
        .parse::<usize>()
        .map_err(|_| invalid_input(format!("{label} must be an integer")))?;
    if !(min..=max).contains(&number) {
        return Err(invalid_input(format!(
            "{label} must be between {min} and {max}"
        )));
    }
    Ok(number)
}

fn search_hit_json(hit: &SearchHit) -> String {
    format!(
        "{{\"evidence_id\":{},\"sources\":[{}],\"excerpt\":{}}}",
        hit.evidence_id,
        hit.source_uris
            .iter()
            .map(|uri| json_string(uri))
            .collect::<Vec<_>>()
            .join(","),
        json_string(&hit.excerpt),
    )
}

fn assessment_json(assessment: &crate::store::AssessmentRecord) -> String {
    format!(
        "{{\"assessment_id\":{},\"vendor\":{},\"name\":{},\"version\":{},\"use_case\":{},\"criteria\":{},\"decision\":{},\"decision_date\":{},\"reviewer\":{},\"state\":{},\"evidence_ids\":[{}]}}",
        assessment.id,
        json_option(assessment.vendor.as_deref()),
        json_string(&assessment.name),
        json_option(assessment.version.as_deref()),
        json_string(&assessment.use_case),
        json_string(&assessment.criteria),
        json_option(assessment.decision.as_deref()),
        json_option(assessment.decision_date.as_deref()),
        json_option(assessment.reviewer.as_deref()),
        json_string(&assessment.state),
        assessment
            .evidence
            .iter()
            .map(|evidence| evidence.id.to_string())
            .collect::<Vec<_>>()
            .join(","),
    )
}

fn format_ingest_receipt(receipt: &crate::store::IngestReceipt) -> String {
    format!(
        "{{\"source_id\":{},\"content_hash\":{},\"evidence_ids\":[{}],\"duplicate\":{},\"review_proposal_id\":{}}}\n",
        receipt.source_id,
        json_string(&receipt.content_hash),
        receipt
            .evidence_ids
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(","),
        receipt.duplicate,
        receipt
            .review_proposal_id
            .map_or_else(|| "null".to_owned(), |id| id.to_string()),
    )
}

fn json_option(value: Option<&str>) -> String {
    value.map_or_else(|| "null".to_owned(), json_string)
}

fn json_string(value: &str) -> String {
    let mut output = String::with_capacity(value.len() + 2);
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character <= '\u{1f}' => {
                output.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

fn json_line(stdout: String) -> CommandOutput {
    CommandOutput {
        stdout,
        stderr: String::new(),
    }
}

fn canonical_failure_uri(input: &str) -> Option<String> {
    crate::ingest::safe_source_uri(input)
}

fn invalid_input(message: impl Into<String>) -> Box<dyn Error + Send + Sync> {
    Box::new(io::Error::new(io::ErrorKind::InvalidInput, message.into()))
}
