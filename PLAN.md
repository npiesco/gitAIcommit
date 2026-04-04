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
  - Next TDD target:
    - move provider-specific startup assumptions fully behind the provider-neutral layer so non-Ollama providers no longer emit Ollama-shaped startup logging like `[START] Starting Ollama...`

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
