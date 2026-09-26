use std::{
    error::Error,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::{
    ingest::{self, Result},
    store::{AssessmentRecord, Store},
};

const MISSING_MARKER: &[u8] = b"VIBERAVEN-ABSENT-TARGET\n";
const README_BEGIN: &str = "<!-- BEGIN VIBERAVEN CATALOGUE -->";
const README_END: &str = "<!-- END VIBERAVEN CATALOGUE -->";
static NEXT_SWAP_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Csv,
    Readme,
}

impl ExportFormat {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "csv" => Ok(Self::Csv),
            "readme" | "markdown" | "md" => Ok(Self::Readme),
            _ => Err(invalid_input("export format must be csv or readme")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    pub content: Vec<u8>,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyReceipt {
    pub target: PathBuf,
    pub backup: PathBuf,
    pub content_sha256: String,
}

pub fn preview(store: &Store, format: ExportFormat) -> Result<Preview> {
    let assessments = store.assessments(true)?;
    let content = match format {
        ExportFormat::Csv => render_csv(&assessments),
        ExportFormat::Readme => render_readme(&assessments),
    };
    Ok(Preview {
        sha256: ingest::hash_bytes(content.as_bytes()),
        content: content.into_bytes(),
    })
}

pub fn target_fingerprint(target: &Path) -> Result<String> {
    let target = normalized_target(target)?;
    match fs::symlink_metadata(&target) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(invalid_input("export target must not be a symbolic link"))
        }
        Ok(metadata) if !metadata.is_file() => {
            Err(invalid_input("export target must be a regular file"))
        }
        Ok(_) => Ok(ingest::hash_bytes(&fs::read(target)?)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok("missing".to_owned()),
        Err(error) => Err(Box::new(error)),
    }
}

pub fn apply(
    store: &Store,
    format: ExportFormat,
    target: &Path,
    expected_target_sha256: &str,
    expected_preview_sha256: &str,
) -> Result<ApplyReceipt> {
    let target = normalized_target(target)?;
    let rendered = preview(store, format)?;
    if rendered.sha256 != expected_preview_sha256 {
        return Err(invalid_input(
            "catalogue changed after preview; create and review a fresh preview",
        ));
    }
    let current = target_fingerprint(&target)?;
    if current != expected_target_sha256 {
        return Err(invalid_input(
            "export destination changed after preview; review the current file before applying",
        ));
    }
    let content = match format {
        ExportFormat::Csv => rendered.content,
        ExportFormat::Readme if current == "missing" => rendered.content,
        ExportFormat::Readme => merge_readme_block(&fs::read(&target)?, &rendered.content)?,
    };
    let content_sha256 = ingest::hash_bytes(&content);
    let backup = create_backup(&target, &current)?;
    let temporary = write_temporary(&target, &content)?;
    match replace_file(&target, &temporary, &current, &content_sha256) {
        Ok(()) => Ok(ApplyReceipt {
            target,
            backup,
            content_sha256,
        }),
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            Err(error)
        }
    }
}

pub fn rollback(target: &Path, backup: &Path, expected_current_sha256: &str) -> Result<PathBuf> {
    let target = normalized_target(target)?;
    if fs::symlink_metadata(backup)?.file_type().is_symlink() {
        return Err(invalid_input("rollback backup must not be a symbolic link"));
    }
    let backup = backup.canonicalize()?;
    let target_parent = target
        .parent()
        .ok_or_else(|| invalid_input("target needs a parent directory"))?;
    if backup.parent() != Some(target_parent) {
        return Err(invalid_input("rollback backup must be beside the target"));
    }
    let expected_prefix = format!(
        "{}.viberaven-backup-",
        target
            .file_name()
            .ok_or_else(|| invalid_input("target needs a file name"))?
            .to_string_lossy()
    );
    if !backup
        .file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with(&expected_prefix))
    {
        return Err(invalid_input(
            "rollback file is not a Viberaven backup for this target",
        ));
    }
    let missing_target = backup
        .extension()
        .is_some_and(|extension| extension == "missing");
    if missing_target && fs::read(&backup)? != MISSING_MARKER {
        return Err(invalid_input("missing-target rollback marker is invalid"));
    }
    let original = if missing_target {
        None
    } else {
        Some(fs::read(&backup)?)
    };
    let current = target_fingerprint(&target)?;
    if current != expected_current_sha256 {
        return Err(invalid_input(
            "export target changed after apply; refusing rollback",
        ));
    }
    let safety_backup = create_backup(&target, &current)?;
    if let Some(original) = original {
        let original_hash = ingest::hash_bytes(&original);
        let temporary = write_temporary(&target, &original)?;
        if let Err(error) = replace_file(&target, &temporary, &current, &original_hash) {
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
        return Ok(safety_backup);
    }
    if current != "missing" {
        remove_if_unchanged(&target, &current)?;
    }
    Ok(safety_backup)
}

fn render_csv(assessments: &[AssessmentRecord]) -> String {
    let mut output = String::from(
        "assessment_id,vendor,product,version,use_case,criteria,decision,decision_date,reviewer,evidence\n",
    );
    for assessment in assessments {
        let evidence = assessment
            .evidence
            .iter()
            .map(|evidence| {
                let sources = evidence.source_uris.join(" | ");
                format!(
                    "evidence:{} {} — {}",
                    evidence.id,
                    sources,
                    one_line(&evidence.text)
                )
            })
            .collect::<Vec<_>>()
            .join(" ; ");
        let fields = [
            assessment.id.to_string(),
            assessment.vendor.clone().unwrap_or_default(),
            assessment.name.clone(),
            assessment.version.clone().unwrap_or_default(),
            assessment.use_case.clone(),
            assessment.criteria.clone(),
            assessment.decision.clone().unwrap_or_default(),
            assessment.decision_date.clone().unwrap_or_default(),
            assessment.reviewer.clone().unwrap_or_default(),
            evidence,
        ];
        output.push_str(
            &fields
                .iter()
                .map(|field| csv_field(field))
                .collect::<Vec<_>>()
                .join(","),
        );
        output.push('\n');
    }
    output
}

fn render_readme(assessments: &[AssessmentRecord]) -> String {
    let mut output = String::from(
        "<!-- BEGIN VIBERAVEN CATALOGUE -->\n\n# Viberaven catalogue\n\nGenerated from human-approved assessments. Evidence links preserve their source references.\n",
    );
    if assessments.is_empty() {
        output.push_str("\nNo approved assessments.\n\n<!-- END VIBERAVEN CATALOGUE -->\n");
        return output;
    }
    for assessment in assessments {
        let product = if let Some(version) = assessment.version.as_deref() {
            format!(
                "{} ({})",
                markdown_escape(&assessment.name),
                markdown_escape(version)
            )
        } else {
            format!("{} (version unknown)", markdown_escape(&assessment.name))
        };
        let heading = assessment
            .vendor
            .as_deref()
            .map_or(product.clone(), |vendor| {
                format!("{} / {product}", markdown_escape(vendor))
            });
        output.push_str(&format!("\n## {heading}\n\n",));
        output.push_str(&format!(
            "- **Use case:** {}\n- **Criteria:** {}\n- **Decision:** {}\n- **Reviewed:** {} by {}\n- **Evidence:**\n",
            markdown_escape(&assessment.use_case),
            markdown_escape(&assessment.criteria),
            markdown_escape(assessment.decision.as_deref().unwrap_or("unknown")),
            assessment.decision_date.as_deref().unwrap_or("unknown"),
            markdown_escape(assessment.reviewer.as_deref().unwrap_or("unknown")),
        ));
        for evidence in &assessment.evidence {
            let sources = evidence
                .source_uris
                .iter()
                .map(|uri| format!("`{}`", markdown_code(uri)))
                .collect::<Vec<_>>()
                .join(", ");
            output.push_str(&format!(
                "  - evidence #{} from {}: {}\n",
                evidence.id,
                sources,
                markdown_escape(&one_line(&evidence.text)),
            ));
        }
    }
    output.push_str("\n<!-- END VIBERAVEN CATALOGUE -->\n");
    output
}

fn merge_readme_block(existing: &[u8], generated: &[u8]) -> Result<Vec<u8>> {
    let existing = std::str::from_utf8(existing)
        .map_err(|_| invalid_input("README export target must be valid UTF-8"))?;
    let generated = std::str::from_utf8(generated)
        .map_err(|_| invalid_input("README preview must be valid UTF-8"))?;
    let begin_positions = existing
        .match_indices(README_BEGIN)
        .map(|(position, _)| position)
        .collect::<Vec<_>>();
    let end_positions = existing
        .match_indices(README_END)
        .map(|(position, _)| position)
        .collect::<Vec<_>>();

    match (begin_positions.as_slice(), end_positions.as_slice()) {
        ([], []) => {
            let mut merged = existing.to_owned();
            if !merged.is_empty() {
                let newline = if merged.ends_with("\r\n") {
                    "\r\n"
                } else {
                    "\n"
                };
                if !merged.ends_with(&format!("{newline}{newline}")) {
                    if merged.ends_with(newline) {
                        merged.push_str(newline);
                    } else {
                        merged.push_str(newline);
                        merged.push_str(newline);
                    }
                }
            }
            merged.push_str(generated);
            Ok(merged.into_bytes())
        }
        ([begin], [end]) if begin < end => {
            let begin_end = begin + README_BEGIN.len();
            let end_after_marker = end + README_END.len();
            let bytes = existing.as_bytes();
            if (*begin > 0 && bytes[begin - 1] != b'\n')
                || !matches!(bytes.get(begin_end), None | Some(b'\r' | b'\n'))
                || (*end > 0 && bytes[end - 1] != b'\n')
                || !matches!(bytes.get(end_after_marker), None | Some(b'\r' | b'\n'))
            {
                return Err(invalid_input(
                    "README catalogue markers must each occupy a full line",
                ));
            }
            let mut replacement_end = end_after_marker;
            if bytes.get(replacement_end) == Some(&b'\r')
                && bytes.get(replacement_end + 1) == Some(&b'\n')
            {
                replacement_end += 2;
            } else if bytes.get(replacement_end) == Some(&b'\n') {
                replacement_end += 1;
            }
            let mut merged = String::with_capacity(existing.len() + generated.len());
            merged.push_str(&existing[..*begin]);
            merged.push_str(generated);
            merged.push_str(&existing[replacement_end..]);
            Ok(merged.into_bytes())
        }
        _ => Err(invalid_input(
            "README export target has missing, duplicate, or reversed catalogue markers",
        )),
    }
}

fn csv_field(value: &str) -> String {
    let value = if value
        .trim_start()
        .chars()
        .next()
        .is_some_and(|character| matches!(character, '=' | '+' | '-' | '@'))
    {
        format!("'{value}")
    } else {
        value.to_owned()
    };
    if value
        .chars()
        .any(|character| matches!(character, ',' | '"' | '\n' | '\r'))
    {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value
    }
}

fn markdown_escape(value: &str) -> String {
    one_line(value)
        .chars()
        .map(|character| {
            if matches!(
                character,
                '\\' | '`'
                    | '*'
                    | '_'
                    | '{'
                    | '}'
                    | '['
                    | ']'
                    | '<'
                    | '>'
                    | '('
                    | ')'
                    | '#'
                    | '+'
                    | '-'
                    | '!'
                    | '|'
            ) {
                format!("\\{character}")
            } else {
                character.to_string()
            }
        })
        .collect()
}

fn markdown_code(value: &str) -> String {
    value.replace('`', "\\`").replace(['\n', '\r'], " ")
}

fn one_line(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalized_target(path: &Path) -> Result<PathBuf> {
    let file_name = path
        .file_name()
        .ok_or_else(|| invalid_input("export target must name a file"))?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .canonicalize()?;
    let target = parent.join(file_name);
    if let Ok(metadata) = fs::symlink_metadata(&target)
        && (metadata.file_type().is_symlink() || !metadata.is_file())
    {
        return Err(invalid_input(
            "export target must be a regular file, not a link or directory",
        ));
    }
    Ok(target)
}

fn create_backup(target: &Path, current_hash: &str) -> Result<PathBuf> {
    let suffix = if current_hash == "missing" {
        "missing"
    } else {
        "bak"
    };
    for attempt in 0..100 {
        let backup = sibling_path(
            target,
            &format!(
                "backup-{}-{}-{attempt}.{suffix}",
                ingest::now_unix(),
                std::process::id()
            ),
        )?;
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&backup)
        {
            Ok(mut file) => {
                let result = (|| -> Result<()> {
                    if current_hash == "missing" {
                        file.write_all(MISSING_MARKER)?;
                    } else {
                        let bytes = fs::read(target)?;
                        if ingest::hash_bytes(&bytes) != current_hash {
                            return Err(invalid_input(
                                "export destination changed while creating its backup",
                            ));
                        }
                        file.write_all(&bytes)?;
                    }
                    file.sync_all()?;
                    Ok(())
                })();
                if let Err(error) = result {
                    drop(file);
                    let _ = fs::remove_file(&backup);
                    return Err(error);
                }
                return Ok(backup);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(Box::new(error)),
        }
    }
    Err(invalid_input("could not allocate a unique rollback file"))
}

fn write_temporary(target: &Path, content: &[u8]) -> Result<PathBuf> {
    for attempt in 0..100 {
        let temporary = sibling_path(
            target,
            &format!(
                "tmp-{}-{}-{attempt}",
                ingest::now_unix(),
                std::process::id()
            ),
        )?;
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(mut file) => {
                if let Err(error) = file.write_all(content).and_then(|()| file.sync_all()) {
                    let _ = fs::remove_file(&temporary);
                    return Err(Box::new(error));
                }
                return Ok(temporary);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(Box::new(error)),
        }
    }
    Err(invalid_input("could not allocate an export temporary file"))
}

fn replace_file(
    target: &Path,
    temporary: &Path,
    expected_hash: &str,
    expected_new_hash: &str,
) -> Result<()> {
    let actual = target_fingerprint(target)?;
    if actual != expected_hash {
        return Err(invalid_input(
            "export target changed while applying the preview",
        ));
    }
    if actual == "missing" {
        fs::rename(temporary, target)?;
        if target_fingerprint(target)? != expected_new_hash {
            return Err(invalid_input(
                "export destination changed while writing the reviewed preview",
            ));
        }
        return Ok(());
    }
    let swap_id = NEXT_SWAP_ID.fetch_add(1, Ordering::Relaxed);
    let displaced = sibling_path(target, &format!("swap-{}-{swap_id}", std::process::id()))?;
    fs::rename(target, &displaced)?;
    if ingest::hash_bytes(&fs::read(&displaced)?) != expected_hash {
        restore_displaced(target, &displaced)?;
        return Err(invalid_input(
            "export destination changed while applying the preview",
        ));
    }
    if path_exists(target) {
        return Err(invalid_input(format!(
            "export destination was recreated during the update; the displaced file remains at {}",
            displaced.display()
        )));
    }
    if let Err(error) = fs::rename(temporary, target) {
        if let Err(restore_error) = restore_displaced(target, &displaced) {
            return Err(invalid_input(format!(
                "export failed ({error}); the original file remains at {} ({restore_error})",
                displaced.display()
            )));
        }
        return Err(Box::new(error));
    }
    if target_fingerprint(target)? != expected_new_hash {
        return Err(invalid_input(format!(
            "export destination changed during the update; the previous file remains at {}",
            displaced.display()
        )));
    }
    let _ = fs::remove_file(displaced);
    Ok(())
}

fn remove_if_unchanged(target: &Path, expected_hash: &str) -> Result<()> {
    if target_fingerprint(target)? != expected_hash {
        return Err(invalid_input(
            "export destination changed after rollback review",
        ));
    }
    let swap_id = NEXT_SWAP_ID.fetch_add(1, Ordering::Relaxed);
    let displaced = sibling_path(target, &format!("swap-{}-{swap_id}", std::process::id()))?;
    fs::rename(target, &displaced)?;
    if ingest::hash_bytes(&fs::read(&displaced)?) != expected_hash {
        restore_displaced(target, &displaced)?;
        return Err(invalid_input(
            "export destination changed while rolling back",
        ));
    }
    if path_exists(target) {
        return Err(invalid_input(format!(
            "export destination was recreated during rollback; the prior file remains at {}",
            displaced.display()
        )));
    }
    fs::remove_file(displaced)?;
    Ok(())
}

fn restore_displaced(target: &Path, displaced: &Path) -> io::Result<()> {
    if path_exists(target) {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "destination now contains a separate concurrent file",
        ));
    }
    fs::rename(displaced, target)
}

fn path_exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn sibling_path(target: &Path, suffix: &str) -> Result<PathBuf> {
    let file_name = target
        .file_name()
        .ok_or_else(|| invalid_input("export target must name a file"))?
        .to_string_lossy();
    let parent = target
        .parent()
        .ok_or_else(|| invalid_input("export target needs a parent directory"))?;
    Ok(parent.join(format!("{file_name}.viberaven-{suffix}")))
}

fn invalid_input(message: impl Into<String>) -> Box<dyn Error + Send + Sync> {
    Box::new(io::Error::new(io::ErrorKind::InvalidInput, message.into()))
}
