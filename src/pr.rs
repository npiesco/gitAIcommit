use anyhow::{bail, Result};
use std::io::Write;
use std::path::Path;
use std::process::Command;
use tempfile::NamedTempFile;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequestDraft {
    pub title: String,
    pub body: String,
}

impl PullRequestDraft {
    pub fn parse_or_normalize(raw: &str) -> Result<Self> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            bail!("generated pull request draft was empty");
        }

        if let Some(parsed) = Self::parse_structured(trimmed) {
            return Ok(parsed);
        }

        let mut lines = trimmed
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty());
        let title = lines
            .next()
            .map(clean_title_line)
            .filter(|line| !line.is_empty())
            .ok_or_else(|| anyhow::anyhow!("generated pull request title was empty"))?;

        let remaining = lines.collect::<Vec<_>>().join("\n");
        let body = if remaining.trim().is_empty() {
            "Generated from the current staged diff summary.".to_string()
        } else {
            remaining.trim().to_string()
        };

        Ok(Self { title, body })
    }

    fn parse_structured(raw: &str) -> Option<Self> {
        let title_idx = raw.find("TITLE:")?;
        let body_idx = raw.find("\nBODY:\n")?;
        if body_idx <= title_idx {
            return None;
        }

        let title = raw[title_idx + "TITLE:".len()..body_idx].trim();
        let body = raw[body_idx + "\nBODY:\n".len()..].trim();

        if title.is_empty() || body.is_empty() {
            return None;
        }

        Some(Self {
            title: clean_title_line(title),
            body: clean_body_text(body),
        })
    }

    pub fn render(&self) -> String {
        format!("TITLE: {}\nBODY:\n{}", self.title, self.body)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PullRequestCreateResult {
    Created { url: String },
    Existing { url: String },
}

fn clean_title_line(value: &str) -> String {
    let trimmed = value.trim().trim_matches('`').trim();
    strip_known_prefix(trimmed, &["TITLE:", "Title:"]).to_string()
}

fn clean_body_text(value: &str) -> String {
    let trimmed = value.trim().trim_matches('`').trim();
    strip_known_prefix(trimmed, &["BODY:", "Body:"]).to_string()
}

fn strip_known_prefix<'a>(value: &'a str, prefixes: &[&str]) -> &'a str {
    for prefix in prefixes {
        if let Some(stripped) = value.strip_prefix(prefix) {
            return stripped.trim();
        }
    }
    value
}

pub fn create_pull_request_via_gh(
    draft: &PullRequestDraft,
    base_branch: &str,
    cwd: &Path,
) -> Result<PullRequestCreateResult> {
    let mut body_file = NamedTempFile::new()?;
    body_file.write_all(draft.body.as_bytes())?;
    body_file.flush()?;

    let output = Command::new("gh")
        .args([
            "pr",
            "create",
            "--title",
            &draft.title,
            "--base",
            base_branch,
            "--body-file",
        ])
        .arg(body_file.path())
        .current_dir(cwd)
        .env("GH_PROMPT_DISABLED", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()?;

    if !output.status.success() {
        if let Some(url) = view_existing_pull_request_url(cwd)? {
            return Ok(PullRequestCreateResult::Existing { url });
        }

        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if !stderr.is_empty() { stderr } else { stdout };
        bail!(
            "gh pr create failed{}",
            if detail.is_empty() {
                String::new()
            } else {
                format!(": {detail}")
            }
        );
    }

    Ok(PullRequestCreateResult::Created {
        url: String::from_utf8_lossy(&output.stdout).trim().to_string(),
    })
}

fn view_existing_pull_request_url(cwd: &Path) -> Result<Option<String>> {
    let output = Command::new("gh")
        .args(["pr", "view", "--json", "url"])
        .current_dir(cwd)
        .env("GH_PROMPT_DISABLED", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()?;

    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_pr_json_url(&stdout))
}

fn parse_pr_json_url(raw: &str) -> Option<String> {
    let marker = "\"url\":";
    let start = raw.find(marker)? + marker.len();
    let rest = raw[start..].trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    let url = &rest[..end];
    if url.trim().is_empty() {
        None
    } else {
        Some(url.to_string())
    }
}
