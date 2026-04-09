# gitAICommit — Gaps to Adopt from crabclaw

Feature gaps identified by comparing `gitAICommit` against `crabclaw`'s commit,
PR, branch, and provider orchestration. Each section now points at the exact
`crabclaw` file/function we can port or adapt, plus the current
`gitAICommit` code that would receive the change.

---

## 1. PR Title + Body Generation

**Gap:** `gitAICommit` generates commit messages only. No PR support exists.

**gitAICommit touch points:**
- `src/main.rs:104-155` — current generate/confirm/commit flow
- `src/formatting/prompt.rs:21-150` — commit-only prompt builder/template

**crabclaw sources to port:**
- `rust/crates/claw-cli/src/main.rs:1708-1738` — `run_pr()`
  - builds a PR prompt from context + diff summary
  - sanitizes output
  - parses title/body
  - shells out to `gh pr create --title ... --body-file ...`
- `rust/crates/claw-cli/src/main.rs:2314-2322` — `parse_titled_body()`
- `rust/crates/claw-cli/src/main.rs:2301-2308` — `truncate_for_prompt()`
- `rust/crates/claw-cli/src/main.rs:2310-2312` — `sanitize_generated_message()`

**Adoption plan:**
- Add `--pr` or a `pr` subcommand to `src/cli/args.rs`
- Add a PR prompt mode to `PromptBuilder`
- Port `parse_titled_body()` nearly verbatim
- Reuse `truncate_for_prompt()` for PR prompt payload limits
- Write PR body to a temp file and call `gh pr create`
- Fallback to printing a draft when `gh` is missing or fails
- Completed TDD slice: `--pr --dry-run` now generates a real PR draft through the existing provider path
  - `tests/interactive_test.rs` — added `test_pr_dry_run_generates_pr_title_and_body_from_repo_context`
  - `tests/cli_args_test.rs` — added `test_pr_flag`
  - `src/cli/args.rs` — added the `--pr` flag
  - `src/formatting/prompt.rs` — added `PromptBuilder::build_pr()` and a `crabclaw`-style `TITLE:` / `BODY:` PR prompt contract built from staged diff summary, recent commit context, and optional user context
  - `src/pr.rs` — added `PullRequestDraft::parse_or_normalize()` so weak local-model output is normalized back into the `TITLE:` / `BODY:` shape instead of inventing a second CLI formatter
  - `src/main.rs` — routes `--pr` through the existing provider-neutral `LlmManager` and prints a PR draft in dry-run mode instead of entering the commit path
  - Leveraged `crabclaw`'s `run_pr()` prompt contract from `/home/npiesco/crabclaw/rust/crates/claw-cli/src/main.rs:2656-2665`
  - Leveraged `crabclaw`'s title/body parsing boundary from `/home/npiesco/crabclaw/rust/crates/claw-cli/src/main.rs:2314-2322`
  - Reused the same truncation philosophy from `/home/npiesco/crabclaw/rust/crates/claw-cli/src/main.rs:2301-2308` through `gitAICommit`'s existing `truncate_for_prompt()` path instead of inventing another limiter
- Completed TDD slice: non-dry-run `--pr` now attempts real `gh pr create` and falls back to the generated draft when creation fails
  - `tests/interactive_test.rs` — added `test_pr_generation_falls_back_to_draft_when_gh_create_fails`
  - `src/pr.rs` — added `create_pull_request_via_gh()` which writes the generated PR body to a real temp file, calls the real `gh pr create --title ... --body-file ...` path, and disables interactive prompting via `GH_PROMPT_DISABLED=1` and `GIT_TERMINAL_PROMPT=0`
  - `src/pr.rs` — tightened `PullRequestDraft` cleanup so repeated `Title:` / `Body:` echoes from weaker local models are stripped at the same parsing boundary instead of leaking into the final draft
  - `src/main.rs` — now attempts real PR creation for `--pr` outside dry-run and falls back to printing the normalized draft, plus an informational `gh pr create failed: ...` message, when the real `gh` command fails in the repository
  - Leveraged `crabclaw`'s real `gh pr create --body-file` flow from `/home/npiesco/crabclaw/rust/crates/claw-cli/src/main.rs:2667-2684` instead of inventing a new executor
- Completed TDD slice: PR generation now threads the detected default branch into the real PR path
  - `tests/interactive_test.rs` — added `test_pr_fallback_surfaces_detected_default_base_branch`, which creates a real bare `origin`, pushes `main`, clones it, switches to a feature branch, runs the actual binary, and verifies the fallback PR output includes `BASE: main`
  - `src/main.rs` — detects the default branch before PR creation and surfaces it in dry-run, success, and fallback PR output
  - `src/pr.rs` — `create_pull_request_via_gh()` now passes `--base <default_branch>` to the real `gh pr create` invocation instead of relying on GitHub CLI defaults
  - Leveraged `crabclaw`'s default-branch detection source from `/home/npiesco/crabclaw/rust/crates/commands/src/lib.rs:1189-1203`
  - Leveraged `crabclaw`'s helper shape from `/home/npiesco/crabclaw/rust/crates/commands/src/lib.rs:1246-1270`
  - Reused `crabclaw`'s real PR creation boundary from `/home/npiesco/crabclaw/rust/crates/claw-cli/src/main.rs:2667-2684`, adapting it to explicitly target the detected base branch
- Completed hardening slice discovered during regression: local OpenAI-compatible responses that return an empty body now fall back to the native Ollama path
  - `src/llm/mod.rs` — `OpenAiCompatibleManager::generate_commit()` now routes empty local OpenAI-compatible responses through `generate_via_ollama_native()` instead of failing with `OpenAI-compatible provider returned an empty response`
  - This keeps the provider boundary aligned with the existing local-Ollama transport handling instead of weakening the real integration tests
- Next TDD target:
  - complete the happy-path PR creation UX by surfacing the created PR URL cleanly, then move on to combined commit+push+PR orchestration

---

## 2. Combined Commit + Push + PR Flow

**Gap:** `gitAICommit` stops after `git commit`.

**gitAICommit touch points:**
- `src/main.rs:126-183` — commit confirmation + `perform_commit()`
- `src/git/collector.rs:232-245` — current branch lookup

**crabclaw sources to port:**
- `rust/crates/commands/src/lib.rs:683-688` — `CommitPushPrRequest`
- `rust/crates/commands/src/lib.rs:810-915` — `handle_commit_push_pr_slash_command()`
- `rust/crates/commands/src/lib.rs:917-935` — `detect_default_branch()`
- `rust/crates/commands/src/lib.rs:1016-1050` — `build_branch_name()` + `slugify()`
- `rust/crates/commands/src/lib.rs:1052-1065` — PR URL parsing helpers
- `rust/crates/commands/src/lib.rs:1006-1013` — `write_temp_text_file()`

**Adoption plan:**
- Add composable `--push`, `--pr`, and `--push-pr` flags
- Extract post-generation workflow out of `src/main.rs`
- Port the commit/push/PR orchestration flow from `handle_commit_push_pr_slash_command()`
- Reuse the branch diff check against `<default>...HEAD`
- Reuse `gh pr view --json url` fallback when PR already exists
- Completed TDD slice: `--push` now commits and pushes the current feature branch to a real local `origin`
  - `tests/interactive_test.rs` — added `test_push_flag_pushes_committed_change_to_real_remote_feature_branch`, which creates a real bare remote, clones it, switches to a feature branch, runs the actual binary, and verifies the pushed remote branch HEAD matches local HEAD
  - `tests/cli_args_test.rs` — added `test_push_flag` and extended combined-flag coverage
  - `src/cli/args.rs` — added the `--push` flag
  - `src/git/repository.rs` — added `push_current_branch()` and promoted `current_branch()` into the reusable git helper layer instead of reimplementing branch lookup inside `main.rs`
  - `src/git/mod.rs` — exports `push_current_branch()` and `current_branch()`
  - `src/main.rs` — after a successful real commit, `--push` now calls the shared git helper and surfaces a deterministic `[PUSH]` result block
  - Leveraged `crabclaw`'s current-branch and push orchestration direction from `/home/npiesco/crabclaw/rust/crates/commands/src/lib.rs:1089-1125`
  - Leveraged `crabclaw`'s helper layer shape from `/home/npiesco/crabclaw/rust/crates/commands/src/lib.rs:1207-1232` instead of adding ad hoc git calls in the CLI flow
- Completed TDD slice: `--push` now auto-creates and switches to a slugified feature branch when invoked from the default branch
  - `tests/interactive_test.rs` — added `test_push_flag_from_default_branch_creates_and_pushes_slugified_feature_branch`, which starts on `main`, runs the actual binary with `--push`, and verifies the flow switched to and pushed a real slugified branch derived from user context
  - `src/git/repository.rs` — added `ensure_push_branch()` plus direct ports of `build_branch_name()` and `slugify()` into the reusable git helper layer
  - `src/git/mod.rs` — exports `ensure_push_branch()`
  - `src/main.rs` — before pushing, the `--push` path now ensures it is off the detected default branch, using user context first and sanitized commit message second as the branch-name hint
  - Leveraged `crabclaw`'s default-branch push safety flow from `/home/npiesco/crabclaw/rust/crates/commands/src/lib.rs:1089-1101`
  - Leveraged `crabclaw`'s branch naming helpers from `/home/npiesco/crabclaw/rust/crates/commands/src/lib.rs:1288-1345` instead of inventing a different slug strategy
- Completed TDD slice: `--push-pr` now runs the real combined commit, push, and PR flow end to end
  - `tests/interactive_test.rs` — added `test_push_pr_from_default_branch_commits_pushes_and_falls_back_to_pr_draft`, which creates a real bare remote, starts on `main`, runs the actual binary with `--push-pr`, verifies a real commit was created, verifies the flow switched to and pushed a real slugified branch, and verifies PR creation falls back to a generated `TITLE:` / `BODY:` draft when `gh` cannot create the PR
  - `tests/cli_args_test.rs` — added `test_push_pr_flag` and extended default/combined flag coverage to include `--push-pr`
  - `src/cli/args.rs` — added the `--push-pr` flag as a first-class combined orchestration path
  - `src/main.rs` — added the real combined flow: generate a commit message, create the commit, reuse `ensure_push_branch()` and `push_current_branch()`, then generate a PR draft and route it through the existing `gh pr create` / fallback path
  - `src/main.rs` — added `generate_sanitized_commit_message()` with a logged retry when a weak local-model response sanitizes to empty, fixing the real regression surfaced in `test_push_flag_pushes_committed_change_to_real_remote_feature_branch` during full-suite validation
  - Leveraged `crabclaw`'s `handle_commit_push_pr_slash_command()` orchestration flow from `/home/npiesco/crabclaw/rust/crates/commands/src/lib.rs:1089-1157` instead of inventing a second commit/push/PR pipeline
  - Reused the already-ported branch safety helpers derived from `/home/npiesco/crabclaw/rust/crates/commands/src/lib.rs:1288-1345`
- Completed TDD slice: `--push-pr` now works when the worktree is clean but the current feature branch is already ahead of the default branch
  - `tests/interactive_test.rs` — added `test_push_pr_with_clean_worktree_uses_existing_branch_commits`, which creates a real bare remote, pushes `main`, creates and commits on a real feature branch, runs the actual binary with `--push-pr` from a clean worktree, and verifies the flow does not exit early as “no changes”, pushes the existing branch, and falls back to a generated PR draft
  - `src/git/repository.rs` — added `branch_diff_stat()`, which computes a real `git diff --stat <default>...HEAD` summary for branch-ahead PR generation
  - `src/git/mod.rs` — exports `branch_diff_stat()`
  - `src/formatting/prompt.rs` — added `build_pr_from_branch_diff()`, which reuses the existing PR prompt contract and truncation path but feeds it branch diff context instead of staged-worktree context
  - `src/main.rs` — before the old empty-worktree early return, the `--push-pr` path now detects default branch and checks whether the current branch is already ahead; if it is, the flow skips commit generation, reuses `push_current_branch()`, and generates the PR draft from branch diff context instead of bailing out
  - `src/main.rs` — added `generate_pull_request_draft()` with a logged retry when a weak local-model response cannot be parsed into `TITLE:` / `BODY:`, fixing the real regression surfaced in `test_pr_fallback_surfaces_detected_default_base_branch` during full-suite validation
  - Leveraged `crabclaw`'s combined orchestration direction from `/home/npiesco/crabclaw/rust/crates/commands/src/lib.rs:1089-1157` instead of inventing a separate clean-worktree PR path
  - Leveraged `crabclaw`'s branch comparison shape from `/home/npiesco/crabclaw/rust/crates/commands/src/lib.rs:1117-1126` by checking branch state against `<default>...HEAD` rather than relying only on staged worktree status
- Completed TDD slice: plain `--pr` now works when the worktree is clean but the current feature branch is already ahead of the default branch
  - `tests/interactive_test.rs` — added `test_pr_dry_run_with_clean_worktree_uses_existing_branch_commits`, which creates a real bare remote, pushes `main`, creates and commits on a real feature branch, runs the actual binary with `--pr --dry-run` from a clean worktree, and verifies the flow generates a PR draft against `main` instead of exiting as “No changes detected in the repository.”
  - `src/main.rs` — the branch-diff PR context gate now applies to plain `--pr` as well as `--push-pr`, so clean branches that are ahead of default reuse the same `branch_diff_stat()` path instead of falling through to the generic empty-worktree exit
  - `src/main.rs` — the plain `--pr` execution path now reuses `build_pr_from_branch_diff()` when branch diff context is available, keeping the prompt contract identical to the already-ported clean-worktree `--push-pr` path instead of inventing a second PR-only prompt
  - Ported the same `crabclaw` branch comparison behavior from `/home/npiesco/crabclaw/rust/crates/commands/src/lib.rs:1117-1126` that already drives the clean-worktree `--push-pr` path, extending it to plain PR generation instead of treating `--pr` as staged-worktree-only
- Completed TDD slice: `--push-pr` now reports the combined-flow skip reason when the worktree is clean and the current branch is not ahead of the default branch
  - `tests/interactive_test.rs` — added `test_push_pr_with_no_branch_changes_reports_combined_skip_reason`, which creates a real bare remote, clones `main`, switches to a real feature branch with no commits ahead, runs the actual binary with `--push-pr`, and verifies the flow surfaces the combined skip reason instead of the generic empty-repository message
  - `src/main.rs` — when `--push-pr` sees no staged/worktree changes and `branch_diff_stat()` also shows no `<default>...HEAD` changes, it now prints `[INFO] No branch changes to push or open as a pull request.` and exits cleanly instead of falling through to the generic “No changes detected in the repository” path
  - Leveraged `crabclaw`'s `handle_commit_push_pr_slash_command()` skip behavior from `/home/npiesco/crabclaw/rust/crates/commands/src/lib.rs:1143-1149` instead of inventing another empty-repository branch in the CLI flow
- Completed TDD slice: `--push-pr --dry-run` now previews the slugified feature branch it would use without mutating the repo
  - `tests/interactive_test.rs` — added `test_push_pr_dry_run_previews_slugified_branch_without_switching`, which creates a real bare remote, runs the actual binary from `main` with staged changes and `--push-pr --dry-run`, verifies the repo stays on `main`, and verifies the output previews the slugified feature branch name instead of incorrectly showing `BRANCH: main`
  - `src/git/repository.rs` — added `preview_push_branch()`, which reuses the same default-branch and branch-name logic as `ensure_push_branch()` but does not switch branches
  - `src/git/mod.rs` — exports `preview_push_branch()`
  - `src/main.rs` — in the combined dry-run path, after commit-message generation, the CLI now previews the branch it would push by calling `preview_push_branch()` with the same context-first / commit-message-second hint that the real push flow already uses
  - Leveraged `crabclaw`'s shared branch naming flow from `/home/npiesco/crabclaw/rust/crates/commands/src/lib.rs:1121-1129` and `/home/npiesco/crabclaw/rust/crates/commands/src/lib.rs:1314-1345` instead of inventing a separate dry-run branch naming rule
- Completed TDD slice: `--push-pr --dry-run` now surfaces the push preview block and branch-action decision explicitly
  - `tests/interactive_test.rs` — extended `test_push_pr_dry_run_previews_slugified_branch_without_switching` so the real CLI must print `[DRY RUN] Push preview:` and `BRANCH ACTION: switched` when running from `main`, while still leaving the repository on `main`
  - `src/main.rs` — in the combined dry-run path, after `preview_push_branch()` resolves the non-mutating target branch, the CLI now prints the same push-preview structure as the real push flow, including `BRANCH ACTION: ready|switched`, `BRANCH: ...`, and `REMOTE: origin`
  - Ported `crabclaw`'s branch-action output shape from `/home/npiesco/crabclaw/rust/crates/commands/src/lib.rs:1206` and continued reusing the shared branch naming flow from `/home/npiesco/crabclaw/rust/crates/commands/src/lib.rs:1121-1129` and `/home/npiesco/crabclaw/rust/crates/commands/src/lib.rs:1314-1345` instead of inventing a separate combined dry-run preview contract
- Completed TDD slice: `--push --dry-run` now previews the slugified feature branch it would use without mutating the repo
  - `tests/interactive_test.rs` — added `test_push_dry_run_previews_slugified_branch_without_switching`, which creates a real bare remote, runs the actual binary from `main` with staged changes and `--push --dry-run`, verifies the repo stays on `main`, and verifies the output previews the slugified feature branch name instead of behaving like a plain commit dry-run
  - `src/main.rs` — in the plain push dry-run path, the CLI now emits a `[DRY RUN] Push preview:` block using `preview_push_branch()` before printing the generated commit message, so the preview matches the real push flow without switching branches
  - Reused the same `preview_push_branch()` helper and therefore the same `crabclaw`-derived branch naming flow from `/home/npiesco/crabclaw/rust/crates/commands/src/lib.rs:1121-1129` and `/home/npiesco/crabclaw/rust/crates/commands/src/lib.rs:1314-1345` instead of inventing a separate dry-run rule for `--push`
- Completed TDD slice: `--push --dry-run` now previews whether the real flow would stay on the current branch or switch to a slugified feature branch
  - `tests/interactive_test.rs` — extended `test_push_dry_run_previews_slugified_branch_without_switching` to verify the real CLI output includes `BRANCH ACTION: switched` when running from `main`, while still leaving the repository on `main`
  - `src/main.rs` — the plain push dry-run preview now compares `current_branch()` to `preview_push_branch()` and prints `BRANCH ACTION: ready` or `BRANCH ACTION: switched`, so the dry-run preview surfaces the same branch transition decision as the real push path
  - Leveraged `crabclaw`'s branch-action output shape from `/home/npiesco/crabclaw/rust/crates/commands/src/lib.rs:1206` while continuing to reuse the shared branch naming flow from `/home/npiesco/crabclaw/rust/crates/commands/src/lib.rs:1121-1129` and `/home/npiesco/crabclaw/rust/crates/commands/src/lib.rs:1314-1345`
- Completed TDD slice: real GitHub-backed `--push-pr` now creates a PR on the first run and surfaces the existing PR on the second run
  - `tests/interactive_test.rs` — added `test_push_pr_with_real_github_repo_creates_then_reuses_pull_request`, which creates a disposable private GitHub repo with `gh`, clones it into `/tmp`, stages a real change, runs the actual binary with `--push-pr`, verifies a real PR is created, then runs the actual binary with `--pr` from the clean feature branch and verifies the existing PR URL is surfaced on the second run
  - `tests/interactive_test.rs` — added `DisposableGithubRepo`, `github_login()`, and the live-URL parsing helpers needed to create and tear down the disposable private repo without introducing stubs or mock GitHub behavior
  - `src/pr.rs` — changed `create_pull_request_via_gh()` to return `PullRequestCreateResult::{Created, Existing}` and ported the `gh pr create` -> `gh pr view --json url` fallback shape so the app can distinguish a newly created PR from an already-open one
  - `src/main.rs` — both the plain `--pr` and combined `--push-pr` paths now surface `[PULL REQUEST] Created pull request:` and `[PULL REQUEST] Existing pull request:` explicitly, including the real PR URL when `gh` returns one
  - `src/main.rs` — fixed the orchestration order the live GitHub test exposed: when `--push` or `--push-pr` starts on the default branch, `ensure_push_branch()` now runs before `commit_message_to_repo()` so the real commit lands on the feature branch instead of on `main`
  - Ported `crabclaw`'s existing-PR fallback behavior from `/home/npiesco/crabclaw/rust/crates/commands/src/lib.rs:1210-1243`, specifically the `gh pr create` failure path that immediately tries `gh pr view --json url`
  - Ported `crabclaw`'s branch-first combined-flow ordering from `/home/npiesco/crabclaw/rust/crates/commands/src/lib.rs:1150-1188`, so branch creation/switching happens before the commit is written when the flow starts on the default branch
- Next TDD target:
  - decide whether the live GitHub-backed test should stay `#[ignore]` by default or be promoted into a dedicated opt-in CI job, now that the full disposable-repo lifecycle is proven end to end

---

## 3. Multiple LLM Provider Support

**Gap:** `gitAICommit` is Ollama-only.

**gitAICommit touch points:**
- `src/main.rs:58-65` — hardwires `OllamaManager`
- `src/cli/args.rs:105-123` — model selection assumes Ollama
- `src/ollama/mod.rs:14-35` — `OllamaClientTrait`
- `src/ollama/client.rs` — Ollama-only HTTP client
- `src/ollama/manager.rs` — Ollama lifecycle management

**crabclaw sources to port:**
- `rust/crates/api/src/providers/mod.rs:12-24` — `Provider` trait
- `rust/crates/api/src/providers/mod.rs:26-32` — `ProviderKind`
- `rust/crates/api/src/providers/mod.rs:144-209` — model aliasing + provider detection
- `rust/crates/api/src/client.rs:21-90` — `ProviderClient` dispatch layer
- `rust/crates/api/src/providers/openai_compat.rs:26-117` — `OpenAiCompatConfig` + `OpenAiCompatClient::{new,from_env,for_ollama}`
- `rust/crates/claw-cli/src/main.rs:1617-1622` — provider-agnostic call site via `run_internal_prompt_text()`

**Adoption plan:**
- Replace `OllamaClientTrait` with a provider-neutral trait/interface
- Add a small `ProviderKind` enum and `ProviderClient`-style dispatch layer
- Keep Ollama as one backend
- Port the OpenAI-compatible client shape for OpenAI and other `/v1/chat/completions` providers
- Move provider/base URL/API key selection into config + env resolution
  - Completed TDD slice: route the real CLI through a provider-neutral manager while preserving Ollama as the default backend
    - `tests/list_models_test.rs` — added `test_list_models_with_explicit_ollama_provider_flag`
    - `src/main.rs` — replaced the top-level `OllamaManager` wiring with `llm::LlmManager`, so `--list-models`, model checks, startup, and generation now go through one provider-neutral entrypoint
    - `src/llm/mod.rs` — added `ProviderKind`, `LlmManagerOptions`, and `LlmManager` with real `Ollama` and `OpenAiCompatible` backends
    - `src/llm/mod.rs` — added an OpenAI-compatible `/v1/models` and `/v1/chat/completions` client shape so the abstraction is real, not a placeholder
    - `src/cli/args.rs` — added `--provider`, `--base-url`, and `--api-key`
    - `src/config.rs` — added persisted `provider`, `base_url`, and `api_key` fields so provider selection is not CLI-only
    - `tests/cli_args_test.rs` and `tests/config_test.rs` — extended coverage for provider/base URL/API key parsing and config loading
    - Leveraged `gitsumintelligence`'s provider-neutral config and enum shape from `/home/npiesco/gitsumintelligence/src/llm/provider.rs` instead of inventing another provider descriptor
    - Leveraged `ailighter`'s factory and OpenAI-compatible client pattern from [/home/npiesco/ailighter/src-tauri/src/llm/factory.rs](/home/npiesco/ailighter/src-tauri/src/llm/factory.rs) and [/home/npiesco/ailighter/src-tauri/src/llm/providers/generic_openai.rs](/home/npiesco/ailighter/src-tauri/src/llm/providers/generic_openai.rs) instead of inventing another HTTP shape
  - Completed TDD slice: `openai-compatible` now honors the local provider port when no explicit base URL is supplied
    - `tests/list_models_test.rs` — added `test_list_models_with_openai_compatible_provider_uses_local_ollama_port`
    - `tests/list_models_test.rs` — boots a real local Ollama process through `OllamaManager::ensure_running()` and then exercises the real `git-ai-commit` binary with `--provider openai-compatible --port 11434 --list-models`
    - `src/llm/mod.rs` — changed `LlmManager::new()` so the `openai-compatible` backend defaults to `http://localhost:{port}` instead of `https://api.openai.com` when `--base-url` is omitted
    - `src/llm/mod.rs` — keeps explicit `--base-url` override behavior intact while making the local-provider path usable for Ollama's `/v1/models` and `/v1/chat/completions` endpoints
    - Leveraged `ailighter`'s local OpenAI-format provider pattern from [/home/npiesco/ailighter/src-tauri/src/llm/factory.rs](/home/npiesco/ailighter/src-tauri/src/llm/factory.rs) and [/home/npiesco/ailighter/src-tauri/src/llm/providers/generic_openai.rs](/home/npiesco/ailighter/src-tauri/src/llm/providers/generic_openai.rs), specifically the idea that a local OpenAI-compatible backend should be addressable from local connection settings rather than hardcoded cloud defaults
  - Completed TDD slice: non-Ollama providers no longer inherit Ollama default-model discovery
    - `tests/cli_args_test.rs` — added `test_openai_compatible_provider_requires_explicit_model`
    - `src/cli/args.rs` — `Args::normalize_provider_defaults()` now clears the implicit Ollama-derived model when `--provider openai-compatible` is selected without an explicit `--model`
    - `src/main.rs` — added a provider-aware runtime guard so non-Ollama providers fail clearly if `--model` is omitted
    - Leveraged `gitsumintelligence`'s provider-aware config semantics from `/home/npiesco/gitsumintelligence/src/llm/provider.rs` and `crabclaw`'s provider dispatch direction from `rust/crates/api/src/providers/mod.rs:144-209` instead of keeping Ollama-specific defaulting logic at the top level
  - Completed TDD slice: end-to-end `openai-compatible` generation now works against the real local Ollama path
    - `tests/interactive_test.rs` — added `test_openai_compatible_provider_generates_commit_message_via_local_ollama`
    - `src/llm/mod.rs` — `OpenAiCompatibleManager::generate_commit()` now tries `/v1/chat/completions` first, then falls back to native Ollama endpoints on local `404` responses
    - `src/llm/mod.rs` — native fallback now supports both `/api/chat` and `/api/generate`, reusing the message-oriented Ollama shape instead of assuming one endpoint
    - `src/ollama/client.rs` — tightened `OllamaClient::is_running()` so only successful `/api/tags` responses count as a healthy Ollama server; `404` no longer fools the provider tests into skipping real startup
    - `tests/interactive_test.rs` and `tests/list_models_test.rs` — real Ollama-backed integration tests now run on a serialized `serial(ollama)` lane and keep the booted `OllamaManager` alive for the duration of the test instead of dropping it immediately
    - Leveraged `crabclaw`'s OpenAI-compatible client boundary from `rust/crates/api/src/providers/openai_compat.rs:26-117`, `gitsumintelligence`'s native Ollama chat request shape from `/home/npiesco/gitsumintelligence/src/llm/providers/ollama.rs`, and `ailighter`'s native Ollama generate path from `/home/npiesco/ailighter/src-tauri/src/llm/providers/ollama.rs` rather than inventing another local-provider compatibility layer
  - Completed TDD slice: provider-specific empty-list guidance for `--list-models` now lives behind the provider-neutral manager
    - `tests/list_models_test.rs` — added `test_list_models_with_openai_compatible_provider_does_not_print_ollama_pull_hint`
    - `src/llm/mod.rs` — added `ModelListing` so `LlmManager::list_models()` returns both model names and provider-aware empty-state guidance instead of a bare `Vec<String>`
    - `src/llm/mod.rs` — `Ollama` keeps the existing `ollama pull <model>` hint, while `OpenAiCompatible` now returns the neutral message `No models found for provider 'openai-compatible'.`
    - `src/main.rs` — the CLI now renders provider-aware empty-state text from `LlmManager` instead of hardcoding an Ollama-only hint at the top level
    - Leveraged `crabclaw`'s provider-dispatch shape from `rust/crates/api/src/client.rs:21-90` and `rust/crates/api/src/providers/mod.rs:144-209` by pushing provider-specific behavior behind the provider-neutral boundary instead of adding another `if provider == ...` branch in `main.rs`
  - Completed TDD slice: provider-specific model readiness now lives behind the provider-neutral manager
    - `tests/interactive_test.rs` — added `test_openai_compatible_provider_fails_early_when_model_is_missing`
    - `src/llm/mod.rs` — `LlmManager::ensure_model_available()` now delegates readiness checks for `openai-compatible` instead of silently no-oping outside Ollama
    - `src/llm/mod.rs` — `OpenAiCompatibleManager::ensure_model_available()` now verifies the selected model against `/v1/models` before generation and fails with `Model '<name>' is not available for provider 'openai-compatible'`
    - `src/llm/mod.rs` — extracted `fetch_model_ids()` so list-models and readiness checks share the same provider-native discovery path instead of duplicating endpoint logic
    - Leveraged `crabclaw`'s provider-neutral dispatch boundary from `rust/crates/api/src/client.rs:21-90` and `rust/crates/api/src/providers/mod.rs:144-209` by moving readiness into the provider layer, not by adding another startup special-case in `main.rs`
  - Completed TDD slice: provider-specific startup messaging now lives behind the provider-neutral manager
    - `tests/interactive_test.rs` — added `test_openai_compatible_provider_does_not_print_ollama_startup_banner`
    - `src/llm/mod.rs` — added `StartupStatus` and `LlmManager::startup_status()` so backend-specific startup messaging is emitted from the provider layer instead of hardcoded at the CLI boundary
    - `src/main.rs` — now prints startup messaging only when the selected provider returns one, which keeps the Ollama banner for the Ollama backend and suppresses it for `openai-compatible`
    - Leveraged `crabclaw`'s provider-neutral dispatch boundary from `rust/crates/api/src/client.rs:21-90` and `rust/crates/api/src/providers/mod.rs:144-209` by moving startup behavior behind the provider layer rather than adding another `if provider == ...` branch in `main.rs`
  - Completed TDD slice: provider-specific model-check messaging now lives behind the provider-neutral manager
    - `tests/interactive_test.rs` — added `test_openai_compatible_provider_does_not_print_ollama_model_check_banner`
    - `src/llm/mod.rs` — added `ReadinessStatus` and `LlmManager::readiness_status()` so backend-specific readiness messaging is emitted from the provider layer instead of hardcoded at the CLI boundary
    - `src/main.rs` — now prints the `[CHECK]` banner only when the selected provider returns one, which keeps the existing Ollama wording for Ollama and suppresses it for `openai-compatible`
    - Leveraged `crabclaw`'s provider-neutral dispatch boundary from `rust/crates/api/src/client.rs:21-90` and `rust/crates/api/src/providers/mod.rs:144-209` by moving readiness wording behind the provider layer rather than adding another `if provider == ...` branch in `main.rs`
  - Completed TDD slice: provider-specific analysis messaging now lives behind the provider-neutral manager
    - `tests/interactive_test.rs` — added `test_openai_compatible_provider_does_not_print_analysis_banner`
    - `src/llm/mod.rs` — added `AnalysisStatus` and `LlmManager::analysis_status()` so backend-specific analysis messaging is emitted from the provider layer instead of hardcoded at the CLI boundary
    - `src/main.rs` — now prints the `[ANALYZE]` banner only when the selected provider returns one, which keeps the existing Ollama wording for Ollama and suppresses it for `openai-compatible`
    - Leveraged `crabclaw`'s provider-neutral dispatch boundary from `rust/crates/api/src/client.rs:21-90` and `rust/crates/api/src/providers/mod.rs:144-209` by moving analysis wording behind the provider layer rather than adding another `if provider == ...` branch in `main.rs`
  - Next TDD target:
    - move the remaining provider-shaped lifecycle wording behind the provider-neutral layer so non-Ollama providers no longer inherit generic CLI banners that should be backend-owned

---

## 4. AI Output Sanitization

**Gap:** `gitAICommit` commits raw model output.

**gitAICommit touch points:**
- `src/main.rs:114-150` — generated text is displayed and committed without cleanup
- `src/main.rs:170-183` — `perform_commit()` uses `git commit -m`

**crabclaw sources to port:**
- `rust/crates/claw-cli/src/main.rs:2310-2312` — `sanitize_generated_message()`
- `rust/crates/claw-cli/src/main.rs:1684-1694` — sanitized commit flow using `git commit --file`

**Adoption plan:**
- Add `sanitize_commit_message()` in `src/main.rs` or a small helper module
- Sanitize before dry-run display and before actual commit
- Prefer `git commit --file <tempfile>` over `git commit -m` so multi-line messages remain intact
- Extend the sanitizer slightly beyond `crabclaw` to strip common LLM preambles if needed

---

## 5. Smarter Diff Context Strategy

**Gap:** `gitAICommit` uses file-name/stat summaries only, with rough line-cost heuristics.

**gitAICommit touch points:**
- `src/formatting/prompt.rs:32-128` — staged/unstaged summary assembly + heuristic truncation
- `src/git/collector.rs:121-157` — collects `git diff --numstat`, not hunks
- `src/git/collector.rs:160-209` — collects file change list

**crabclaw sources to port:**
- `rust/crates/claw-cli/src/main.rs:1709-1715` — uses `git diff --stat` in prompt construction
- `rust/crates/claw-cli/src/main.rs:2301-2308` — `truncate_for_prompt()`

**Adoption plan:**
- Keep `gitAICommit`'s staged/unstaged grouping and file tagging
- Port `truncate_for_prompt()` for deterministic character-bound truncation
- Add an optional hunk mode that collects real staged diff content
- Sort/prioritize files by change magnitude before truncation
- Preserve config/test tagging in the prompt output

---

## 6. Default Branch Detection

**Gap:** `gitAICommit` does not know the repository default branch.

**gitAICommit touch points:**
- `src/git/collector.rs:232-245` — only current branch detection exists
- future push/PR workflow in `src/main.rs`

**crabclaw sources to port:**
- `rust/crates/commands/src/lib.rs:917-935` — `detect_default_branch()`
- `rust/crates/commands/src/lib.rs:974-995` — `branch_exists()` + `current_branch()`

**Adoption plan:**
- Add a `git::detect_default_branch()` utility
- Use it for PR base selection
- Use it for deciding whether to auto-create a feature branch
- Use it for the "no branch changes" diff check

- Completed TDD slice: added real default-branch detection through a local bare `origin` clone
  - `src/git/repository.rs` — added `detect_default_branch()` plus the narrow `git_stdout()`, `branch_exists()`, and `current_branch()` helpers
  - `src/git/mod.rs` — exports `detect_default_branch`
  - `tests/interactive_test.rs` — added `test_detect_default_branch_prefers_origin_head_from_real_remote`, which creates a real bare remote, pushes a real `main` branch, clones it, switches to a feature branch, and verifies detection still returns `main`
  - Leveraged `crabclaw`'s current `detect_default_branch()` flow from `/home/npiesco/crabclaw/rust/crates/commands/src/lib.rs:1189-1203`
  - Leveraged `crabclaw`'s helper shape from `/home/npiesco/crabclaw/rust/crates/commands/src/lib.rs:1246-1270`
- Next TDD target: thread detected default branch into PR creation so `gh pr create` targets the repository base branch explicitly instead of relying on GitHub CLI defaults

---

## 7. Auto Branch Name Generation

**Gap:** `gitAICommit` has no branch management.

**gitAICommit touch points:**
- future push/PR workflow in `src/main.rs`
- possibly a new helper in `src/git/`

**crabclaw sources to port:**
- `rust/crates/commands/src/lib.rs:1016-1050` — `build_branch_name()` + `slugify()`

**Adoption plan:**
- Port `slugify()` directly
- Adapt `build_branch_name()` to use commit subject or PR title as the hint
- Add `--branch <name>` override
- Only auto-create when current branch equals detected default branch

---

## 8. Custom Template Support (Fix Existing Gap)

**Gap:** `--template` exists but is ignored.

**gitAICommit touch points:**
- `src/cli/args.rs:183-195` — `template: Option<PathBuf>`
- `src/main.rs:61` — `PromptBuilder::new(...)` receives no template input
- `src/formatting/prompt.rs:11-18` — constructor always loads `default_template()`
- `src/formatting/prompt.rs:130-150` — hardcoded default template

**crabclaw sources to port/adapt:**
- No direct template-loader exists in `crabclaw`
- Best reusable pattern is limited to prompt string assembly:
  - `rust/crates/claw-cli/src/main.rs:1708-1715` — build prompt from structured parts
  - `rust/crates/claw-cli/src/main.rs:2301-2308` — trim prompt payloads before sending

**Adoption plan:**
- Change `PromptBuilder::new(...)` to accept an optional template string/path
- Load `--template` in `src/main.rs` before building the prompt
- Keep `{CONTEXT}` as the required placeholder in external templates
- Optionally support named templates later, but first fix the broken file-path case

---

## 9. Test Coverage Gaps

**Gap:** core git/CLI paths remain lightly tested.

**gitAICommit undertested code:**
- `src/git/collector.rs:77-293` — `GitCollector`
- `src/git/status.rs:14-55` — `GitStatus::parse()`
- `src/git/diff.rs:20-57` — `DiffInfo::parse()`
- `src/git/files.rs:24-76` — `FileChange::parse_list()` / `parse_line()`
- `src/main.rs:71-155` — staging, dry-run, confirm, commit flow
- `src/ollama/manager.rs` — lifecycle behavior

**crabclaw sources to port:**
- `rust/crates/commands/src/lib.rs:1669-1748` — temp repo helpers: `temp_dir()`, `env_lock()`, `run_command()`, `init_git_repo()`, `init_bare_repo()`
- `rust/crates/commands/src/lib.rs:1747-1760` — `write_fake_gh()`
- `rust/crates/commands/src/lib.rs:2421-2510` — real repo integration tests for commit and commit+push+PR
- `rust/crates/api/src/client.rs:128-147` and `rust/crates/api/src/providers/mod.rs:221-245` — simple provider selection unit test style

**Adoption plan:**
- Add a shared test helper module for temp git repos
- Port the fake `gh` binary pattern for future PR tests
- Add parser tests for rename/copy/unmerged cases
- Add integration tests for `--dry-run`, `--add-unstaged`, and real commit creation
- Set `user.name`/`user.email` inside test repos explicitly

---

## 10. User-Supplied Context in Prompts

**Gap:** `gitAICommit` has no explicit way to capture intent beyond git metadata.

**gitAICommit touch points:**
- `src/cli/args.rs` — add a new `--context <TEXT>` flag
- `src/formatting/prompt.rs:22-99` — include user context in prompt rendering

**crabclaw sources to port/adapt:**
- `rust/crates/claw-cli/src/main.rs:2273-2299` — `recent_user_context()`
- `rust/crates/claw-cli/src/main.rs:1710-1714` — PR prompt includes `Context hint: ...`
- `rust/crates/claw-cli/src/main.rs:1680-1684` — commit prompt includes extra context text

**Adoption plan:**
- Add a plain `--context "what changed and why"` flag
- Render that context ahead of diff data in the prompt
- Keep this simple; `crabclaw`'s session history logic is not directly portable, but the prompt shape is

---

## Priority Order

| Priority | Feature | Effort | Impact | Best crabclaw source |
|----------|---------|--------|--------|----------------------|
| P0 | Fix `--template` | S | Medium | `claw-cli/src/main.rs:1708-1715` |
| P0 | AI output sanitization | S | High | `claw-cli/src/main.rs:2310-2312` |
| P1 | Test coverage gaps | M | High | `commands/src/lib.rs:1669-1760`, `2421-2510` |
| P1 | Multiple LLM providers | L | High | `api/src/providers/mod.rs:12-24`, `api/src/client.rs:21-90` |
| P2 | PR title+body generation | M | High | `claw-cli/src/main.rs:1708-1738`, `2314-2322` |
| P2 | Default branch detection | S | Medium | `commands/src/lib.rs:917-935` |
| P2 | `--context` flag | S | Medium | `claw-cli/src/main.rs:1680-1684`, `2273-2299` |
| P3 | Combined commit+push+PR flow | L | High | `commands/src/lib.rs:810-915` |
| P3 | Auto branch name generation | S | Medium | `commands/src/lib.rs:1016-1050` |
| P3 | Smarter diff context | M | Medium | `claw-cli/src/main.rs:2301-2308` |
| P3 | Commit message style parity with `crabclaw` | M | High | `claw-cli/src/main.rs:1680-1715`, `2301-2312` |

---

## Progress Tree

- [x] P0 Fix `--template`
  - Added a real file-backed template loading path in `gitAICommit`:
    - `src/formatting/prompt.rs` — added `PromptBuilder::from_template_file()` and validated `{CONTEXT}` placeholder usage
    - `src/main.rs` — now routes `args.template` into the prompt builder instead of always using `PromptBuilder::new(...)`
  - Red test:
    - `tests/prompt_builder_test.rs` — `test_prompt_builder_uses_custom_template_file`
  - Validation:
    - focused red/green test passed
    - full `cargo test` passed
    - `cargo fmt --all` passed
    - `cargo clippy --all-targets --all-features -- -D warnings` passed
    - second full `cargo test` passed after lint fixes
    - `cargo build --release` passed
  - Porting note:
    - leveraged `crabclaw` prompt assembly shape from `npiesco/crabclaw/rust/crates/claw-cli/src/main.rs:1708-1715`
    - reused the existing `gitAICommit` `{CONTEXT}` substitution path rather than inventing a second prompt system

- [x] P0 AI output sanitization
  - Added a shared sanitized commit path in `gitAICommit`:
    - `src/commit.rs` — added `sanitize_commit_message()` and `commit_message_to_repo()`
    - `src/lib.rs` — exported the new commit helpers for integration tests and reuse
    - `src/main.rs` — now sanitizes generated text before preview/dry-run and commits via `git commit --file`
  - Red test:
    - `tests/commit_sanitization_test.rs` — `test_commit_message_is_sanitized_before_commit`
  - Validation:
    - focused red/green test passed
    - full `cargo test` passed
    - `cargo fmt --all` passed
    - `cargo clippy --all-targets --all-features -- -D warnings` passed
    - second full `cargo test` passed after lint
    - `cargo build --release` passed
  - Porting note:
    - leveraged `crabclaw` sanitizer behavior from `npiesco/crabclaw/rust/crates/claw-cli/src/main.rs:2310-2312` — `sanitize_generated_message()`
    - leveraged `crabclaw`'s temp-file commit flow from `npiesco/crabclaw/rust/crates/claw-cli/src/main.rs:1684-1694` instead of keeping `git commit -m`
    - extended that port narrowly to strip common LLM preambles like `Here is the commit message:` because the red integration test covered that real failure mode
  - Remaining gap:
    - this ports sanitization, not full message-style parity
    - `gitAICommit` still lacks `crabclaw`'s tighter prompt shaping and smarter diff/context trimming, so model output can still drift into chatty or overlong commit text
    - exact `crabclaw` sources still to port for style consistency:
      - `npiesco/crabclaw/rust/crates/claw-cli/src/main.rs:1680-1715` — commit prompt shaping and structured context assembly
      - `npiesco/crabclaw/rust/crates/claw-cli/src/main.rs:2301-2308` — smarter diff truncation before generation

- [ ] P1 Test coverage gaps
  - Completed TDD slice: real staged rename coverage
    - `tests/interactive_test.rs` — added `test_collect_all_reports_staged_rename_with_old_and_new_paths`
    - `src/git/status.rs` — fixed `GitStatus::parse()` so porcelain rename/copy entries record the destination path in `staged_files`
  - Completed TDD slice: merge conflicts count as real repository changes
    - `tests/interactive_test.rs` — added `test_collect_all_treats_merge_conflicts_as_non_empty_changes`
    - `src/git/status.rs` — extended `GitStatus::parse()` to capture porcelain unmerged states in `unmerged_files`
    - `src/git/collector.rs` — updated `GitInfo::is_empty()` so conflicted repos are not treated as empty
  - Completed TDD slice: deduplicate diff stats when one file has both staged and unstaged edits
    - `tests/interactive_test.rs` — added `test_collect_all_deduplicates_diff_stats_for_staged_and_unstaged_same_file`
    - `src/git/diff.rs` — updated `DiffInfo::parse()` to merge repeated filenames into one aggregated `FileStat` entry before computing totals
  - Completed TDD slice: display untracked files only once in repository analysis output
    - `tests/interactive_test.rs` — added `test_git_info_display_lists_untracked_files_once`
    - `src/git/collector.rs` — updated `GitInfo::display()` to rely on `GitStatus::display()` for untracked output instead of appending a duplicate section
  - Completed TDD slice: `--add-unstaged` stages deleted-only repos in the real CLI flow
    - `tests/interactive_test.rs` — added `test_add_unstaged_flag_stages_deleted_files_in_main_flow`
    - `src/main.rs` — updated the `--add-unstaged` gate so deleted files trigger `stage_all_unstaged()` alongside modified and untracked files
  - Completed TDD slice: detect staged copies with original and new paths
    - `tests/interactive_test.rs` — added `test_collect_all_reports_staged_copy_with_old_and_new_paths`
    - `src/git/collector.rs` — updated `get_file_changes()` to request git rename/copy detection with `--find-copies-harder`, so staged copies are emitted as `C...` records instead of plain adds
    - `src/git/files.rs` — reused the existing `FileChange::parse_line()` support for `ChangeType::Copied` and `old_path` preservation rather than introducing a second copy parser
  - Validation:
    - focused red/green test passed
    - full `cargo test` passed
    - `cargo fmt --all` passed
    - `cargo clippy --all-targets --all-features -- -D warnings` passed
    - second full `cargo test` passed after lint
    - `cargo build --release` passed
  - Porting note:
    - leveraged `crabclaw`'s real temp-repo integration testing style from `npiesco/crabclaw/rust/crates/commands/src/lib.rs:1669-1748`, specifically the helper-driven repo setup pattern behind `temp_dir()`, `env_lock()`, `run_command()`, and `init_git_repo()`
    - followed the same behavior-first repo test shape used by `npiesco/crabclaw/rust/crates/commands/src/lib.rs:2421-2510` instead of adding another synthetic parser-only test
- [ ] P1 Multiple LLM providers
- [ ] P2 PR title+body generation
- [ ] P2 Default branch detection
- [ ] P2 `--context` flag
- [ ] P3 Combined commit+push+PR flow
- [ ] P3 Auto branch name generation
- [ ] P3 Smarter diff context
- [ ] P3 Commit message style parity with `crabclaw`
  - Goal:
    - make generated commit messages consistently match `crabclaw`'s tighter style constraints instead of only sanitizing obviously bad output afterward
  - Completed TDD slice: prefer the conventional-commit line from chatty AI output
    - `tests/commit_sanitization_test.rs` — added `test_commit_message_prefers_conventional_commit_from_chatty_output`
    - `src/commit.rs` — extended `sanitize_commit_message()` to extract the first conventional-commit-style subject line from mixed prose output before writing the commit
    - `src/commit.rs` — added narrow conventional-commit detection so `fix: ...` and `type(scope): ...` subjects are kept while explanatory chatter is dropped
  - Completed TDD slice: default prompt uses staged-only commit context
    - `tests/interactive_test.rs` — added `test_prompt_uses_staged_diff_summary_and_excludes_unstaged_details`
    - `tests/interactive_test.rs` — updated the older prompt integration assertion to match staged-only commit behavior
    - `tests/prompt_builder_test.rs` — updated prompt-builder expectations from the old broad template to the narrower staged-only commit contract
    - `src/formatting/prompt.rs` — changed `PromptBuilder::build()` to include only staged file changes and staged diff statistics in the default commit prompt
    - `src/formatting/prompt.rs` — replaced the old generic template with a tighter `crabclaw`-style commit instruction focused on staged diff summary input
  - Completed TDD slice: mark truncated staged prompt context explicitly
    - `tests/interactive_test.rs` — added `test_prompt_marks_truncated_staged_context_when_limits_are_exceeded`
    - `src/formatting/prompt.rs` — updated `add_file_changes_to_context()` to append `…[truncated]` when the staged prompt context exceeds configured limits
    - `src/formatting/prompt.rs` — kept the existing structured section truncation flow, but aligned the truncation signal with `crabclaw` so prompt consumers can see that input was intentionally shortened
  - Completed TDD slice: deterministically truncate staged prompt context by character budget
    - `tests/interactive_test.rs` — added `test_prompt_truncates_single_oversized_staged_entry_by_character_budget`
    - `src/formatting/prompt.rs` — replaced the rough staged entry size heuristic with a `truncate_for_prompt()`-style character-budget truncation pass over the rendered staged context
    - `src/formatting/prompt.rs` — kept the explicit `…[truncated]` marker and made oversized single entries truncate correctly instead of slipping past the budget because they counted as only one estimated file
  - Completed TDD slice: remove extra default prompt scaffolding while preserving richer custom-template context
    - `tests/prompt_builder_test.rs` — added `test_default_prompt_avoids_extra_instruction_block_and_repo_metadata_scaffolding`
    - `src/formatting/prompt.rs` — removed the extra `Requirements:` block from the built-in default commit template so it matches `crabclaw`'s tighter `run_commit()` contract more closely
    - `src/formatting/prompt.rs` — stopped prepending `Current branch:` and `Last commit:` metadata in the built-in default prompt path, but preserved that richer context for `PromptBuilder::from_template_file(...)` and other custom-template renders
    - `tests/prompt_builder_test.rs` — updated the older default-prompt assertions to reflect the narrower built-in contract while keeping the custom-template metadata assertion intact
  - Completed TDD slice: explicit `--context` flag feeds a bounded conversation-context lane into the real CLI prompt flow
    - `tests/interactive_test.rs` — added `test_context_flag_is_included_in_default_prompt_via_real_cli_flow`
    - `tests/cli_args_test.rs` — added `test_context_flag` and extended the combined-options parse coverage
    - `src/cli/args.rs` — added `--context <TEXT>` as a first-class customization flag
    - `src/main.rs` — routed `args.context` into the default prompt builder path without changing custom-template rendering
    - `src/formatting/prompt.rs` — added an explicit `Recent conversation context:` lane for the built-in default prompt when user context is provided, bounded by the same prompt truncation helper
  - Completed TDD slice: default prompt now follows `crabclaw`'s compact staged diff-stat contract instead of injecting extra staged-file scaffolding
    - `tests/prompt_builder_test.rs` — added `test_default_prompt_uses_diff_stat_contract_without_extra_staged_file_scaffolding`
    - `tests/prompt_builder_test.rs` — updated older default-prompt assertions so they validate compact staged diff-stat lines instead of the pre-port `Staged changes (will be committed):` block
    - `tests/interactive_test.rs` — updated `test_prompt_prefers_staged_changes_only` to assert staged diff summary plus staged filenames without reintroducing the removed scaffolding
    - `src/formatting/prompt.rs` — split built-in default prompt context from custom-template context so only the default path adopts the tighter `crabclaw` contract
    - `src/formatting/prompt.rs` — changed the built-in default prompt to render staged diff summary plus compact per-file stat lines (`path | +N -M`) and to truncate that block on line boundaries with the existing `…[truncated]` marker
    - `src/formatting/prompt.rs` — preserved the richer staged file/change sections for custom templates so `--template` users still get the fuller `{CONTEXT}` payload
  - Completed TDD slice: prose-only weak-model output now collapses to a single subject line instead of committing the whole paragraph block
    - `tests/commit_sanitization_test.rs` — added `test_commit_message_falls_back_to_first_meaningful_line_when_model_returns_prose`
    - `src/commit.rs` — kept conventional-commit extraction as the primary path, but added a fallback to the first meaningful non-empty line when no conventional subject is present
    - `src/commit.rs` — preserved the temp-file `git commit --file` flow; the change stays strictly in the sanitizer boundary and does not invent a new commit pipeline
  - Completed TDD slice: malformed single-line weak-model label echoes are stripped before commit
    - `tests/commit_sanitization_test.rs` — added `test_commit_message_strips_malformed_single_line_prompt_labels`
    - `src/commit.rs` — added a narrow single-line cleanup pass that strips malformed `... commit message:` prefixes and leading bracketed prompt-label remnants before the existing fallback logic runs
    - `src/commit.rs` — kept conventional-commit extraction and first-line fallback unchanged as the primary decision path; this only removes weak-model prompt-label noise at the sanitizer boundary
  - Completed TDD slice: glossary-style all-caps weak-model prefixes are stripped before commit
    - `tests/commit_sanitization_test.rs` — added `test_commit_message_strips_glossary_style_all_caps_prefixes`
    - `src/commit.rs` — extended the single-line cleanup pass with `strip_glossary_style_prefix()` so outputs like `LORE (Logical OR Evaluator) [PRINCIPLE #3]: ...` collapse to the usable subject instead of being committed verbatim
    - `src/commit.rs` — kept this as a narrow sanitizer-boundary extension on top of the existing conventional-commit extraction and first-meaningful-line fallback, matching `crabclaw`'s final-write cleanup placement rather than inventing a new generation path
  - Completed TDD slice: trailing echoed prompt scaffolding is stripped from single-line conventional subjects
    - `tests/commit_sanitization_test.rs` — added `test_commit_message_strips_trailing_echoed_prompt_scaffolding_from_single_line_subject`
    - `src/commit.rs` — extended the single-line cleanup path with `strip_trailing_echoed_prompt_scaffolding()` so outputs like `feat: ... - [Commit Message]: ...` keep the conventional subject and drop the echoed prompt tail
    - `src/commit.rs` — kept the change at the same final sanitizer boundary used by `crabclaw`'s `sanitize_generated_message()` in `npiesco/crabclaw/rust/crates/claw-cli/src/main.rs:2310-2312`, instead of inventing another prompt or generation flow
  - Completed TDD slice: bracket-only boilerplate is rejected instead of being committed
    - `tests/commit_sanitization_test.rs` — added `test_commit_message_rejects_bracket_only_boilerplate_after_sanitization`
    - `src/commit.rs` — added `looks_like_bracket_only_boilerplate()` and made `strip_single_line_prompt_labels()` collapse single-line bracket-only outputs like `[Lorem Ipsum]` to empty before conventional-subject extraction
    - `src/commit.rs` — reused the existing `generated commit message was empty` guard in `commit_message_to_repo()` so the real commit path now fails cleanly instead of creating a garbage commit, keeping the fix at the same final-write boundary `crabclaw` uses in `npiesco/crabclaw/rust/crates/claw-cli/src/main.rs:2310-2312`
  - Completed TDD slice: raw git commit-hash metadata is rejected instead of being committed
    - `tests/commit_sanitization_test.rs` — added `test_commit_message_rejects_raw_git_commit_hash_metadata`
    - `src/commit.rs` — added `looks_like_raw_git_metadata()` and made `strip_single_line_prompt_labels()` collapse single-line outputs like `commit 3d89b7c2f5a6...` to empty before fallback selection
    - `src/commit.rs` — reused the same `generated commit message was empty` guard in `commit_message_to_repo()`, preserving `crabclaw`'s final-write sanitizer boundary from `npiesco/crabclaw/rust/crates/claw-cli/src/main.rs:2310-2312` instead of adding another prompt-time workaround
  - Completed TDD slice: literal `git commit -m` shell wrappers are rejected instead of being committed
    - `tests/commit_sanitization_test.rs` — added `test_commit_message_rejects_literal_git_commit_shell_wrapper`
    - `src/commit.rs` — added `looks_like_git_commit_shell_wrapper()` and made `strip_single_line_prompt_labels()` collapse single-line outputs like `git commit -m "..."` to empty before fallback selection
    - `src/commit.rs` — reused the same `generated commit message was empty` guard in `commit_message_to_repo()`, preserving `crabclaw`'s final-write sanitizer boundary from `npiesco/crabclaw/rust/crates/claw-cli/src/main.rs:2310-2312` instead of adding another generation path
  - Primary `gitAICommit` touch points:
    - `src/formatting/prompt.rs` — tighten prompt instructions and output contract
    - `src/main.rs` — keep the generation path wired through the stricter prompt builder
    - `src/commit.rs` — preserve sanitizer as a safety net, not the primary style control
  - `crabclaw` files/functions to leverage instead of reinventing:
    - `npiesco/crabclaw/rust/crates/claw-cli/src/main.rs:1680-1715` — commit prompt construction path
    - `npiesco/crabclaw/rust/crates/claw-cli/src/main.rs:1795` — exact default `run_commit()` prompt wording centered on staged diff summary input
    - `npiesco/crabclaw/rust/crates/claw-cli/src/main.rs:1862-1867` — `run_commit()` uses only `git diff --cached --stat` plus recent context, which is the compact default prompt contract now mirrored in `gitAICommit`
    - `npiesco/crabclaw/rust/crates/claw-cli/src/main.rs:1795` and `1842-1849` — `Recent conversation context:` lane shape and bounded context injection
    - `npiesco/crabclaw/rust/crates/claw-cli/src/main.rs:2301-2308` — `truncate_for_prompt()` and its explicit `…[truncated]` marker used before generation
    - `npiesco/crabclaw/rust/crates/claw-cli/src/main.rs:2310-2312` — sanitizer remains the last-line cleanup layer
  - Next TDD target:
    - tighten sanitizer handling for remaining weak-model outputs that echo literal shell command wrappers like `git commit -m "..."` as the entire generated "commit message"
  - Validation:
    - focused red/green test passed
    - full `cargo test` passed
    - `cargo fmt --all` passed
    - `cargo clippy --all-targets --all-features -- -D warnings` passed
    - second full `cargo test` passed after lint
    - `cargo build --release` passed
