use git_ai_commit::commit::commit_message_to_repo;
use std::process::Command;
use tempfile::tempdir;

fn run_git(repo: &std::path::Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git command should run");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("git stdout should be utf8")
}

#[tokio::test]
async fn test_commit_message_is_sanitized_before_commit() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let repo = temp_dir.path();

    run_git(repo, &["init"]);
    run_git(repo, &["config", "user.name", "GitAICommit Tests"]);
    run_git(repo, &["config", "user.email", "tests@git-ai-commit.local"]);

    std::fs::write(repo.join("README.md"), "seed\n").expect("failed to write seed file");
    run_git(repo, &["add", "README.md"]);
    run_git(repo, &["commit", "-m", "chore: seed repo"]);

    std::fs::write(repo.join("feature.txt"), "new feature\n").expect("failed to write feature");
    run_git(repo, &["add", "feature.txt"]);

    let raw_message = "```text\r\nHere is the commit message:\nfix: add feature\n```";
    commit_message_to_repo(raw_message, repo)
        .await
        .expect("sanitized commit should succeed");

    let committed_message = run_git(repo, &["log", "-1", "--pretty=%B"]);
    assert_eq!(committed_message.trim(), "fix: add feature");
}

#[tokio::test]
async fn test_commit_message_prefers_conventional_commit_from_chatty_output() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let repo = temp_dir.path();

    run_git(repo, &["init"]);
    run_git(repo, &["config", "user.name", "GitAICommit Tests"]);
    run_git(repo, &["config", "user.email", "tests@git-ai-commit.local"]);

    std::fs::write(repo.join("README.md"), "seed\n").expect("failed to write seed file");
    run_git(repo, &["add", "README.md"]);
    run_git(repo, &["commit", "-m", "chore: seed repo"]);

    std::fs::write(repo.join("collector.rs"), "conflict handling\n")
        .expect("failed to write feature");
    run_git(repo, &["add", "collector.rs"]);

    let raw_message = "Here is a concise commit message based on the staged changes:\n\nfix: keep conflicted repos visible after staging\n\nThis updates the repository emptiness check so merge conflicts are not treated as no-op state.";
    commit_message_to_repo(raw_message, repo)
        .await
        .expect("chatty generated output should still produce a clean commit message");

    let committed_message = run_git(repo, &["log", "-1", "--pretty=%B"]);
    assert_eq!(
        committed_message.trim(),
        "fix: keep conflicted repos visible after staging",
        "the committed message should keep the conventional-commit line and drop explanatory prose"
    );
}

#[tokio::test]
async fn test_commit_message_drops_prompt_label_boilerplate_from_echoed_output() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let repo = temp_dir.path();

    run_git(repo, &["init"]);
    run_git(repo, &["config", "user.name", "GitAICommit Tests"]);
    run_git(repo, &["config", "user.email", "tests@git-ai-commit.local"]);

    std::fs::write(repo.join("README.md"), "seed\n").expect("failed to write seed file");
    run_git(repo, &["add", "README.md"]);
    run_git(repo, &["commit", "-m", "chore: seed repo"]);

    std::fs::write(repo.join("cleanup.rs"), "staged cleanup\n").expect("failed to write feature");
    run_git(repo, &["add", "cleanup.rs"]);

    let raw_message = "Generate a git commit message in plain text Lore format only. Base it on this staged diff summary:\n\nStaged changes (will be committed):\n  - modified: cleanup.rs\n\nStaged diff summary: 1 files changed, 4 insertions(+), 1 deletions(-)\n\nRecent conversation context:\nfocus on cleanup before release\n\nCommit Message:\nfix: tighten prompt context handling";
    commit_message_to_repo(raw_message, repo)
        .await
        .expect("echoed prompt boilerplate should still produce a clean commit message");

    let committed_message = run_git(repo, &["log", "-1", "--pretty=%B"]);
    assert_eq!(
        committed_message.trim(),
        "fix: tighten prompt context handling",
        "the committed message should drop echoed prompt labels and keep only the commit subject"
    );
}

#[tokio::test]
async fn test_commit_message_falls_back_to_first_meaningful_line_when_model_returns_prose() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let repo = temp_dir.path();

    run_git(repo, &["init"]);
    run_git(repo, &["config", "user.name", "GitAICommit Tests"]);
    run_git(repo, &["config", "user.email", "tests@git-ai-commit.local"]);

    std::fs::write(repo.join("README.md"), "seed\n").expect("failed to write seed file");
    run_git(repo, &["add", "README.md"]);
    run_git(repo, &["commit", "-m", "chore: seed repo"]);

    std::fs::write(repo.join("prompt.rs"), "prompt tightening\n").expect("failed to write feature");
    run_git(repo, &["add", "prompt.rs"]);

    let raw_message = "Migrating to Lorre format from Lore:\n\nLore: This is a plain text commit message in the Lore format. It has been migrated from the Lore staged diff summary, which was also truncated due to its size.";
    commit_message_to_repo(raw_message, repo)
        .await
        .expect("prose-only model output should still collapse to a single subject line");

    let committed_message = run_git(repo, &["log", "-1", "--pretty=%B"]);
    assert_eq!(
        committed_message.trim(),
        "Migrating to Lorre format from Lore:",
        "when no conventional-commit line exists, the sanitizer should fall back to the first meaningful line instead of committing the full prose block"
    );
}

#[tokio::test]
async fn test_commit_message_strips_malformed_single_line_prompt_labels() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let repo = temp_dir.path();

    run_git(repo, &["init"]);
    run_git(repo, &["config", "user.name", "GitAICommit Tests"]);
    run_git(repo, &["config", "user.email", "tests@git-ai-commit.local"]);

    std::fs::write(repo.join("README.md"), "seed\n").expect("failed to write seed file");
    run_git(repo, &["add", "README.md"]);
    run_git(repo, &["commit", "-m", "chore: seed repo"]);

    std::fs::write(repo.join("feature.rs"), "new feature\n").expect("failed to write feature");
    run_git(repo, &["add", "feature.rs"]);

    let raw_message =
        "Lorig commit message: [Stageed Diff Summary] Adds new feature XYZ for better user experience.";
    commit_message_to_repo(raw_message, repo).await.expect(
        "malformed single-line weak-model output should still sanitize to a usable subject",
    );

    let committed_message = run_git(repo, &["log", "-1", "--pretty=%B"]);
    assert_eq!(
        committed_message.trim(),
        "Adds new feature XYZ for better user experience.",
        "the sanitizer should drop malformed prompt-label prefixes from single-line output instead of committing them verbatim"
    );
}

#[tokio::test]
async fn test_commit_message_strips_glossary_style_all_caps_prefixes() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let repo = temp_dir.path();

    run_git(repo, &["init"]);
    run_git(repo, &["config", "user.name", "GitAICommit Tests"]);
    run_git(repo, &["config", "user.email", "tests@git-ai-commit.local"]);

    std::fs::write(repo.join("README.md"), "seed\n").expect("failed to write seed file");
    run_git(repo, &["add", "README.md"]);
    run_git(repo, &["commit", "-m", "chore: seed repo"]);

    std::fs::write(repo.join("logic.rs"), "boolean logic\n").expect("failed to write feature");
    run_git(repo, &["add", "logic.rs"]);

    let raw_message =
        "LORE (Logical OR Evaluator) [PRINCIPLE #3]: Given two logical expressions, determine if either of them is true.";
    commit_message_to_repo(raw_message, repo).await.expect(
        "glossary-style all-caps weak-model output should still sanitize to a usable subject",
    );

    let committed_message = run_git(repo, &["log", "-1", "--pretty=%B"]);
    assert_eq!(
        committed_message.trim(),
        "Given two logical expressions, determine if either of them is true.",
        "the sanitizer should drop glossary-style all-caps prefixes instead of committing them verbatim"
    );
}

#[tokio::test]
async fn test_commit_message_strips_trailing_echoed_prompt_scaffolding_from_single_line_subject() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let repo = temp_dir.path();

    run_git(repo, &["init"]);
    run_git(repo, &["config", "user.name", "GitAICommit Tests"]);
    run_git(repo, &["config", "user.email", "tests@git-ai-commit.local"]);

    std::fs::write(repo.join("README.md"), "seed\n").expect("failed to write seed file");
    run_git(repo, &["add", "README.md"]);
    run_git(repo, &["commit", "-m", "chore: seed repo"]);

    std::fs::write(repo.join("release.rs"), "release cleanup\n").expect("failed to write feature");
    run_git(repo, &["add", "release.rs"]);

    let raw_message = "feat: tighten release cleanup handling - [Commit Message]: Implemented release cleanup handling based on the staged diff summary.";
    commit_message_to_repo(raw_message, repo).await.expect(
        "single-line output with a valid subject and trailing prompt scaffolding should still sanitize to a clean commit subject",
    );

    let committed_message = run_git(repo, &["log", "-1", "--pretty=%B"]);
    assert_eq!(
        committed_message.trim(),
        "feat: tighten release cleanup handling",
        "the sanitizer should keep the conventional subject and drop trailing echoed prompt scaffolding from the same line"
    );
}

#[tokio::test]
async fn test_commit_message_rejects_bracket_only_boilerplate_after_sanitization() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let repo = temp_dir.path();

    run_git(repo, &["init"]);
    run_git(repo, &["config", "user.name", "GitAICommit Tests"]);
    run_git(repo, &["config", "user.email", "tests@git-ai-commit.local"]);

    std::fs::write(repo.join("README.md"), "seed\n").expect("failed to write seed file");
    run_git(repo, &["add", "README.md"]);
    run_git(repo, &["commit", "-m", "chore: seed repo"]);

    std::fs::write(repo.join("placeholder.rs"), "placeholder cleanup\n")
        .expect("failed to write feature");
    run_git(repo, &["add", "placeholder.rs"]);

    let error = commit_message_to_repo("[Lorem Ipsum]", repo)
        .await
        .expect_err("bracket-only boilerplate should be rejected instead of committed");

    let error_text = error.to_string();
    assert!(
        error_text.contains("generated commit message was empty"),
        "expected sanitized bracket-only boilerplate to fail as empty, got: {error_text}"
    );

    let committed_message = run_git(repo, &["log", "-1", "--pretty=%B"]);
    assert_eq!(
        committed_message.trim(),
        "chore: seed repo",
        "rejecting bracket-only boilerplate should leave HEAD on the seed commit"
    );
}

#[tokio::test]
async fn test_commit_message_rejects_raw_git_commit_hash_metadata() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let repo = temp_dir.path();

    run_git(repo, &["init"]);
    run_git(repo, &["config", "user.name", "GitAICommit Tests"]);
    run_git(repo, &["config", "user.email", "tests@git-ai-commit.local"]);

    std::fs::write(repo.join("README.md"), "seed\n").expect("failed to write seed file");
    run_git(repo, &["add", "README.md"]);
    run_git(repo, &["commit", "-m", "chore: seed repo"]);

    std::fs::write(repo.join("metadata.rs"), "metadata cleanup\n")
        .expect("failed to write feature");
    run_git(repo, &["add", "metadata.rs"]);

    let error = commit_message_to_repo("commit 3d89b7c2f5a6e47744bf6b8921e3cf703dce8c4a", repo)
        .await
        .expect_err("raw git metadata should be rejected instead of committed");

    let error_text = error.to_string();
    assert!(
        error_text.contains("generated commit message was empty"),
        "expected raw git metadata to sanitize to empty, got: {error_text}"
    );

    let committed_message = run_git(repo, &["log", "-1", "--pretty=%B"]);
    assert_eq!(
        committed_message.trim(),
        "chore: seed repo",
        "rejecting raw git metadata should leave HEAD on the seed commit"
    );
}

#[tokio::test]
async fn test_commit_message_rejects_literal_git_commit_shell_wrapper() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let repo = temp_dir.path();

    run_git(repo, &["init"]);
    run_git(repo, &["config", "user.name", "GitAICommit Tests"]);
    run_git(repo, &["config", "user.email", "tests@git-ai-commit.local"]);

    std::fs::write(repo.join("README.md"), "seed\n").expect("failed to write seed file");
    run_git(repo, &["add", "README.md"]);
    run_git(repo, &["commit", "-m", "chore: seed repo"]);

    std::fs::write(repo.join("wrapper.rs"), "wrapper cleanup\n").expect("failed to write feature");
    run_git(repo, &["add", "wrapper.rs"]);

    let error = commit_message_to_repo(
        "git commit -m \"Lorem Ipsum is simply dummy text of the printing and typesetting industry.\"",
        repo,
    )
    .await
    .expect_err("literal git commit shell wrappers should be rejected instead of committed");

    let error_text = error.to_string();
    assert!(
        error_text.contains("generated commit message was empty"),
        "expected literal git commit shell wrapper to sanitize to empty, got: {error_text}"
    );

    let committed_message = run_git(repo, &["log", "-1", "--pretty=%B"]);
    assert_eq!(
        committed_message.trim(),
        "chore: seed repo",
        "rejecting literal git commit shell wrappers should leave HEAD on the seed commit"
    );
}
