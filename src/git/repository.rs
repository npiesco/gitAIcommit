use crate::utils::error::GitAiError;
use anyhow::Result;
use std::env;
use std::path::Path;
use tokio::process::Command;

pub async fn detect_default_branch(repo_path: &Path) -> Result<String> {
    if let Ok(reference) =
        git_stdout(repo_path, &["symbolic-ref", "refs/remotes/origin/HEAD"]).await
    {
        if let Some(branch) = reference
            .trim()
            .rsplit('/')
            .next()
            .filter(|value| !value.is_empty())
        {
            return Ok(branch.to_string());
        }
    }

    for branch in ["main", "master"] {
        if branch_exists(repo_path, branch).await? {
            return Ok(branch.to_string());
        }
    }

    current_branch(repo_path).await
}

async fn git_stdout(repo_path: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .output()
        .await?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(GitAiError::Git(format!("git {} failed: {}", args.join(" "), error)).into());
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

async fn git_status_ok(repo_path: &Path, args: &[&str]) -> Result<()> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if stderr.is_empty() { stdout } else { stderr };
        return Err(GitAiError::Git(format!("git {} failed: {}", args.join(" "), detail)).into());
    }

    Ok(())
}

async fn branch_exists(repo_path: &Path, branch: &str) -> Result<bool> {
    let output = Command::new("git")
        .args([
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ])
        .current_dir(repo_path)
        .output()
        .await?;

    Ok(output.status.success())
}

pub async fn current_branch(repo_path: &Path) -> Result<String> {
    let branch = git_stdout(repo_path, &["branch", "--show-current"]).await?;
    if branch.is_empty() {
        Err(GitAiError::Git("unable to determine current git branch".to_string()).into())
    } else {
        Ok(branch)
    }
}

pub async fn push_current_branch(repo_path: &Path) -> Result<String> {
    let branch = current_branch(repo_path).await?;
    git_status_ok(repo_path, &["push", "--set-upstream", "origin", &branch]).await?;
    Ok(branch)
}

pub async fn branch_diff_stat(repo_path: &Path, base_branch: &str) -> Result<String> {
    git_stdout(
        repo_path,
        &["diff", "--stat", &format!("{base_branch}...HEAD")],
    )
    .await
}

pub async fn ensure_push_branch(repo_path: &Path, hint: &str) -> Result<String> {
    let default_branch = detect_default_branch(repo_path).await?;
    let current = current_branch(repo_path).await?;
    if current != default_branch {
        return Ok(current);
    }

    let next_branch = build_branch_name(hint);
    git_status_ok(repo_path, &["switch", "-c", &next_branch]).await?;
    Ok(next_branch)
}

pub async fn preview_push_branch(repo_path: &Path, hint: &str) -> Result<String> {
    let default_branch = detect_default_branch(repo_path).await?;
    let current = current_branch(repo_path).await?;
    if current != default_branch {
        return Ok(current);
    }

    Ok(build_branch_name(hint))
}

fn build_branch_name(hint: &str) -> String {
    let slug = slugify(hint);
    let owner = env::var("SAFEUSER")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            env::var("USER")
                .ok()
                .filter(|value| !value.trim().is_empty())
        });

    match owner {
        Some(owner) => format!("{owner}/{slug}"),
        None => slug,
    }
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut last_was_dash = false;

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            slug.push('-');
            last_was_dash = true;
        }
    }

    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "change".to_string()
    } else {
        slug
    }
}
