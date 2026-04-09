# GitAICommit

Rust CLI for AI-assisted git commits, pushes, and pull requests.

It analyzes the current repository state, builds a constrained prompt from the relevant diff context, and runs that through a local or compatible LLM backend to generate:

- commit messages
- pull request titles and bodies
- combined commit + push + PR flows

## Current Status

The README now reflects the code that is implemented and validated in this repo:

- provider-neutral LLM manager with `ollama` and `openai-compatible`
- custom prompt templates with `--template`
- explicit user context with `--context`
- real `--push`, `--pr`, and `--push-pr` flows
- default-branch detection
- auto feature-branch creation when pushing from the default branch
- `gh pr create` integration with existing-PR fallback via `gh pr view --json url`

## Prerequisites

- Rust 1.70+
- Git
- One of:
  - Ollama
  - an OpenAI-compatible HTTP endpoint
- `gh` if you want real PR creation instead of dry-run PR drafts

## Installation

```bash
git clone https://github.com/npiesco/gitAICommit.git
cd gitAICommit
cargo install --path .
```

## Quick Start

Basic commit flow:

```bash
git-ai-commit
```

Stage everything first:

```bash
git-ai-commit --add-unstaged
```

Preview only:

```bash
git-ai-commit --dry-run --verbose
```

Generate a PR draft instead of a commit:

```bash
git-ai-commit --pr --dry-run
```

Commit and push:

```bash
git-ai-commit --push
```

Commit, push, and open or reuse a PR:

```bash
git-ai-commit --push-pr
```

## Providers

### Ollama

Default provider:

```bash
git-ai-commit --provider ollama --model gemma4:latest
```

List local models:

```bash
git-ai-commit --list-models
```

### OpenAI-Compatible

Use any OpenAI-compatible endpoint:

```bash
git-ai-commit \
  --provider openai-compatible \
  --model your-model-name \
  --base-url http://localhost:11434
```

With an API key:

```bash
git-ai-commit \
  --provider openai-compatible \
  --model gpt-4.1-mini \
  --base-url https://api.openai.com \
  --api-key "$OPENAI_API_KEY"
```

Notes:

- `--model` is required for non-Ollama providers.
- If `--provider openai-compatible` is used without `--base-url`, the tool defaults to `http://localhost:<port>`.

## CLI Options

```text
git-ai-commit [OPTIONS]

Model options:
    --provider <PROVIDER>          LLM backend: ollama | openai-compatible
    -m, --model <MODEL>            Model name
        --base-url <URL>           Base URL for selected provider
        --api-key <KEY>            API key for selected provider
        --list-models              List models for the selected provider
    -p, --port <PORT>              Ollama port [default: 11434]

Diff options:
    -f, --max-files <COUNT>        Max files included in analysis [default: 10]
    -l, --max-diff-lines <LINES>   Max diff lines / prompt budget unit [default: 50]

Commit options:
    -a, --add-unstaged             Stage unstaged changes first
        --pr                       Generate PR title/body instead of a commit
        --push                     Push after commit
        --push-pr                  Commit, push, then open/reuse PR
        --confirm                  Ask before committing

Customization:
        --template <FILE>          Custom prompt template
        --context <TEXT>           Extra user context for commit/PR generation

Debug:
    -d, --dry-run                  Preview without mutating git state
    -v, --verbose                  Print generated prompt/output details

Advanced:
    -t, --timeout-seconds <SEC>    Generation timeout [default: 60]
```

## Behavior

### Commit Generation

- default commit prompts use staged diff summary context
- AI output is sanitized before commit
- the final commit is written through `git commit --file`
- malformed prompt echoes and shell-wrapper junk are stripped before write

### Push Behavior

- if you run `--push` or `--push-pr` from a non-default branch, that branch is reused
- if you run from the detected default branch, the tool creates a slugified feature branch first
- branch naming uses user `--context` first, then the sanitized commit message

### PR Behavior

- `--pr --dry-run` prints a generated `TITLE:` / `BODY:` draft
- non-dry-run PR flows call `gh pr create --base <default_branch>`
- if `gh pr create` fails because a PR already exists, the tool falls back to `gh pr view --json url`
- clean worktrees are supported when the current branch is already ahead of the default branch

## Custom Templates

Use a custom prompt template file:

```bash
git-ai-commit --template ./prompt.txt
```

The built-in default prompt is intentionally constrained, but custom templates still receive the richer `{CONTEXT}` payload from the repo analysis path.

## Configuration

Config file path:

```text
~/.config/git-ai-commit/config.toml
```

Example:

```toml
provider = "ollama"
model = "gemma4:latest"
base_url = "http://localhost:11434"
api_key = ""
max_files = 10
max_diff_lines = 50
port = 11434
timeout_seconds = 60
```

Notes:

- CLI flags override config values.
- For `openai-compatible`, set `model` explicitly in config or on the command line.

## Examples

Commit with explicit context:

```bash
git-ai-commit --context "focus on cleanup before release"
```

Preview a PR from an existing feature branch:

```bash
git-ai-commit --pr --dry-run --context "user-facing behavior change"
```

Commit, push, and open/reuse a PR through local Ollama OpenAI-compatible mode:

```bash
git-ai-commit \
  --push-pr \
  --provider openai-compatible \
  --model tinyllama:latest \
  --base-url http://localhost:11434
```

## Testing

Local test loop:

```bash
cargo test
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
```

Live GitHub-backed integration test:

```bash
cargo test test_push_pr_with_real_github_repo_creates_then_reuses_pull_request --test interactive_test -- --ignored
```

That test:

1. creates a disposable private GitHub repo with `gh`
2. clones it into `/tmp`
3. runs the real `git-ai-commit` PR flow
4. verifies PR creation and existing-PR reuse
5. deletes the repo on teardown

## License

MIT
