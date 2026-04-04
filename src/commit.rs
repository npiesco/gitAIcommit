use crate::utils::error::GitAiError;
use anyhow::Result;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn sanitize_commit_message(value: &str) -> String {
    let normalized = value.trim().replace("\r\n", "\n");
    let mut sanitized = if normalized.starts_with("```") {
        let lines: Vec<&str> = normalized.lines().collect();
        if lines.len() >= 2 && lines.last().is_some_and(|line| line.trim() == "```") {
            lines[1..lines.len() - 1].join("\n").trim().to_string()
        } else {
            normalized.trim_matches('`').trim().to_string()
        }
    } else {
        normalized.trim_matches('`').trim().to_string()
    };

    if let Some(stripped) = sanitized
        .strip_prefix("Here is the commit message:")
        .or_else(|| sanitized.strip_prefix("Here is your commit message:"))
    {
        sanitized = stripped.trim().to_string();
    }

    sanitized = strip_single_line_prompt_labels(&sanitized);

    extract_conventional_commit_line(&sanitized)
        .or_else(|| first_meaningful_line(&sanitized))
        .unwrap_or(sanitized)
}

fn extract_conventional_commit_line(value: &str) -> Option<String> {
    value
        .lines()
        .map(str::trim)
        .find(|line| looks_like_conventional_commit(line))
        .map(ToOwned::to_owned)
}

fn first_meaningful_line(value: &str) -> Option<String> {
    value
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToOwned::to_owned)
}

fn strip_single_line_prompt_labels(value: &str) -> String {
    let mut cleaned = value.trim().to_string();

    if !cleaned.contains('\n') {
        if looks_like_bracket_only_boilerplate(&cleaned) {
            return String::new();
        }

        if looks_like_raw_git_metadata(&cleaned) {
            return String::new();
        }

        if looks_like_git_commit_shell_wrapper(&cleaned) {
            return String::new();
        }

        if let Some(stripped) = strip_glossary_style_prefix(&cleaned) {
            cleaned = stripped;
        }

        if let Some((prefix, remainder)) = cleaned.split_once(':') {
            if prefix.to_ascii_lowercase().contains("commit message") {
                cleaned = remainder.trim().to_string();
            }
        }

        while let Some(stripped) = strip_bracketed_prefix(&cleaned) {
            cleaned = stripped;
        }

        if let Some(stripped) = strip_trailing_echoed_prompt_scaffolding(&cleaned) {
            cleaned = stripped;
        }
    }

    cleaned
}

fn strip_glossary_style_prefix(value: &str) -> Option<String> {
    let (prefix, remainder) = value.split_once(':')?;
    let prefix = prefix.trim();
    if prefix.is_empty() {
        return None;
    }

    let leading_token = prefix
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_matches(|ch: char| !ch.is_ascii_alphanumeric());
    let leading_token_is_all_caps = !leading_token.is_empty()
        && leading_token.chars().any(|ch| ch.is_ascii_uppercase())
        && leading_token
            .chars()
            .filter(|ch| ch.is_ascii_alphabetic())
            .all(|ch| ch.is_ascii_uppercase());
    let looks_like_glossary =
        leading_token_is_all_caps && (prefix.contains('(') || prefix.contains('['));

    let remainder = remainder.trim();
    if looks_like_glossary && !remainder.is_empty() {
        Some(remainder.to_string())
    } else {
        None
    }
}

fn strip_bracketed_prefix(value: &str) -> Option<String> {
    let trimmed = value.trim_start();
    if !trimmed.starts_with('[') {
        return None;
    }

    let end = trimmed.find(']')?;
    let remainder = trimmed[end + 1..].trim_start();
    if remainder.is_empty() {
        None
    } else {
        Some(remainder.to_string())
    }
}

fn strip_trailing_echoed_prompt_scaffolding(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let markers = [" - [Commit Message]:", " [Commit Message]:"];

    for marker in markers {
        if let Some((subject, _)) = trimmed.split_once(marker) {
            let subject = subject.trim();
            if looks_like_conventional_commit(subject) {
                return Some(subject.to_string());
            }
        }
    }

    None
}

fn looks_like_bracket_only_boilerplate(value: &str) -> bool {
    let trimmed = value.trim();
    if !(trimmed.starts_with('[') && trimmed.ends_with(']')) {
        return false;
    }

    let inner = trimmed
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or_default()
        .trim();

    !inner.is_empty()
}

fn looks_like_raw_git_metadata(value: &str) -> bool {
    let trimmed = value.trim();
    let Some(hash) = trimmed.strip_prefix("commit ") else {
        return false;
    };

    let hash = hash.trim();
    hash.len() >= 7 && hash.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn looks_like_git_commit_shell_wrapper(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.starts_with("git commit -m ")
}

fn looks_like_conventional_commit(line: &str) -> bool {
    if line.is_empty() || line.contains('`') {
        return false;
    }

    let Some((commit_type, remainder)) = line.split_once(':') else {
        return false;
    };

    let commit_type = commit_type.trim();
    let description = remainder.trim();
    if description.is_empty() {
        return false;
    }

    let Some(base_type) = commit_type
        .split_once('(')
        .map(|(prefix, _)| prefix)
        .or(Some(commit_type))
    else {
        return false;
    };

    matches!(
        base_type,
        "feat" | "fix" | "docs" | "style" | "refactor" | "test" | "chore"
    )
}

fn write_temp_commit_message(contents: &str) -> Result<std::path::PathBuf> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let path = std::env::temp_dir().join(format!("git-ai-commit-message-{nanos}.txt"));
    std::fs::write(&path, contents)?;
    Ok(path)
}

pub async fn commit_message_to_repo(raw_message: &str, repo_path: &Path) -> Result<()> {
    let sanitized = sanitize_commit_message(raw_message);
    if sanitized.trim().is_empty() {
        return Err(GitAiError::Git("generated commit message was empty".to_string()).into());
    }

    let message_path = write_temp_commit_message(&sanitized)?;
    let output = tokio::process::Command::new("git")
        .args(["commit", "--file"])
        .arg(&message_path)
        .current_dir(repo_path)
        .output()
        .await?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        return Err(GitAiError::Git(format!("Git commit failed: {}", error)).into());
    }

    Ok(())
}
