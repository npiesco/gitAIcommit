use crate::config::Config;
use crate::ollama::client::OllamaClient;
use crate::ollama::OllamaClientTrait;
use clap::Parser;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use tokio;
use tokio::runtime::Runtime;

// Track which fields were explicitly set via command line
thread_local! {
    static PROVIDER_WAS_SET: AtomicBool = const { AtomicBool::new(false) };
    static MODEL_WAS_SET: AtomicBool = const { AtomicBool::new(false) };
    static MAX_FILES_WAS_SET: AtomicBool = const { AtomicBool::new(false) };
    static MAX_DIFF_LINES_WAS_SET: AtomicBool = const { AtomicBool::new(false) };
    static PORT_WAS_SET: AtomicBool = const { AtomicBool::new(false) };
    static TIMEOUT_WAS_SET: AtomicBool = const { AtomicBool::new(false) };
}

// Helper function to track when a value is set
fn track_value<T>(value: T, flag: &'static std::thread::LocalKey<AtomicBool>) -> T {
    flag.with(|f| f.store(true, Ordering::Relaxed));
    value
}

fn reset_tracked_flags() {
    PROVIDER_WAS_SET.with(|f| f.store(false, Ordering::Relaxed));
    MODEL_WAS_SET.with(|f| f.store(false, Ordering::Relaxed));
    MAX_FILES_WAS_SET.with(|f| f.store(false, Ordering::Relaxed));
    MAX_DIFF_LINES_WAS_SET.with(|f| f.store(false, Ordering::Relaxed));
    PORT_WAS_SET.with(|f| f.store(false, Ordering::Relaxed));
    TIMEOUT_WAS_SET.with(|f| f.store(false, Ordering::Relaxed));
}

fn option_was_explicitly_set(args: &[OsString], long: &str, short: &str) -> bool {
    args.iter().skip(1).any(|arg| {
        let value = arg.to_string_lossy();
        value == long
            || value == short
            || value.starts_with(&format!("{long}="))
            || (short.len() == 2 && value.starts_with(short) && value.len() > short.len())
    })
}

fn sync_tracked_flags_from_args(args: &[OsString]) {
    PROVIDER_WAS_SET.with(|f| {
        f.store(
            option_was_explicitly_set(args, "--provider", ""),
            Ordering::Relaxed,
        )
    });
    MODEL_WAS_SET.with(|f| {
        f.store(
            option_was_explicitly_set(args, "--model", "-m"),
            Ordering::Relaxed,
        )
    });
    MAX_FILES_WAS_SET.with(|f| {
        f.store(
            option_was_explicitly_set(args, "--max-files", "-f"),
            Ordering::Relaxed,
        )
    });
    MAX_DIFF_LINES_WAS_SET.with(|f| {
        f.store(
            option_was_explicitly_set(args, "--max-diff-lines", "-l"),
            Ordering::Relaxed,
        )
    });
    PORT_WAS_SET.with(|f| {
        f.store(
            option_was_explicitly_set(args, "--port", "-p"),
            Ordering::Relaxed,
        )
    });
    TIMEOUT_WAS_SET.with(|f| {
        f.store(
            option_was_explicitly_set(args, "--timeout-seconds", "-t"),
            Ordering::Relaxed,
        )
    });
}

static DEFAULT_MODEL: OnceLock<String> = OnceLock::new();

fn get_default_model() -> String {
    if let Some(model) = DEFAULT_MODEL.get() {
        return model.clone();
    }

    // Check if we're already in a runtime
    if tokio::runtime::Handle::try_current().is_ok() {
        // We're in a runtime, use the current runtime
        let client = OllamaClient::new(11434);

        // Use tokio::task::block_in_place to safely block the current thread
        let last_model = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                if client.is_running().await {
                    client.get_last_model().await.unwrap_or(None)
                } else {
                    None
                }
            })
        });

        if let Some(model) = last_model {
            let _ = DEFAULT_MODEL.set(model.clone());
            return model;
        }
    } else {
        // Not in a runtime, create a new one
        let rt = match Runtime::new() {
            Ok(rt) => rt,
            Err(_) => return "llama3.2".to_string(),
        };

        let client = OllamaClient::new(11434);

        // Check if Ollama is running and get the last model
        let last_model = rt.block_on(async {
            if client.is_running().await {
                client.get_last_model().await.unwrap_or(None)
            } else {
                None
            }
        });

        if let Some(model) = last_model {
            let _ = DEFAULT_MODEL.set(model.clone());
            return model;
        }
    }

    // Fallback to a default model if no models are available or Ollama is not running
    "gemma4:latest".to_string()
}

/// Command-line arguments for git-ai-commit
///
/// This tool generates AI-assisted commit messages and PR drafts by analyzing git changes.
/// It supports provider-aware commit, push, and pull request flows.
#[derive(Parser, Debug)]
#[command(
    name = "git-ai-commit",
    about = "Generate AI-assisted commit messages, pushes, and pull requests",
    long_about = "\
Generate meaningful commit messages and pull request drafts by analyzing your git changes with AI.\n\n\
USAGE EXAMPLES:\n\n\
  # Basic usage (staged changes only)\n  $ git-ai-commit\n\n\
  # Stage all changes before committing\n  $ git-ai-commit --add-unstaged\n\n\
  # Preview changes without committing\n  $ git-ai-commit --dry-run\n\n\
  # Use a specific provider and model\n  $ git-ai-commit --provider ollama --model gemma4:latest\n\n\
  # Use an OpenAI-compatible local endpoint\n  $ git-ai-commit --provider openai-compatible --model tinyllama:latest --base-url http://localhost:11434\n\n\
  # Generate a pull request draft instead of a commit\n  $ git-ai-commit --pr --dry-run\n\n\
  # Commit, push, and open or reuse a PR\n  $ git-ai-commit --push-pr\n\n\
  # Show verbose output for debugging\n  $ git-ai-commit --verbose\n\n\
  # Use a custom prompt template\n  $ git-ai-commit --template ./my-prompt.txt\n\n\
  # Increase diff context for better messages\n  $ git-ai-commit --max-files 20 --max-diff-lines 100\n\n\
  # Run against a local provider on a custom port\n  $ git-ai-commit --port 12345\n\n\
For more information on each option, use --help.",
    version,
    propagate_version = true
)]
pub struct Args {
    /// LLM provider backend to use
    ///
    /// Supported values:
    ///   --provider ollama
    ///   --provider openai-compatible
    #[arg(
        long,
        default_value = "ollama",
        value_name = "PROVIDER",
        help_heading = "Model Options",
        value_parser = |s: &str| {
            let s = s.to_string();
            Ok::<_, std::convert::Infallible>(track_value(s, &PROVIDER_WAS_SET))
        }
    )]
    pub provider: String,

    /// AI model to use for commit message or PR generation
    ///
    /// For Ollama, if not specified, the tool will use the value from the config file,
    /// or fall back to the last model used with Ollama, or `gemma4:latest` as the final fallback.
    /// For non-Ollama providers, pass `--model` explicitly or set it in config.
    ///
    /// Examples:
    ///   --model llama3
    ///   -m mistral
    #[arg(
        short,
        long,
        default_value_t = get_default_model(),
        hide_default_value = true,
        value_name = "MODEL",
        value_parser = |s: &str| {
            let s = s.to_string();
            Ok::<_, std::convert::Infallible>(track_value(s, &MODEL_WAS_SET))
        }
    )]
    pub model: String,

    /// Base URL for the selected provider
    ///
    /// For OpenAI-compatible providers this should be the server root,
    /// for example `https://api.openai.com` or `http://localhost:11434`.
    #[arg(long, value_name = "URL", help_heading = "Model Options")]
    pub base_url: Option<String>,

    /// API key for the selected provider
    ///
    /// This is optional for local providers such as Ollama.
    #[arg(long, value_name = "KEY", help_heading = "Model Options")]
    pub api_key: Option<String>,

    /// Maximum number of files to include in the diff analysis
    ///
    /// Limits the number of files processed to prevent very large diffs.
    /// Files are processed in the order they appear in git status.
    ///
    /// Example:
    ///   --max-files 20
    #[arg(
        short = 'f',
        long,
        default_value = "10",
        value_name = "COUNT",
        help_heading = "Diff Options",
        value_parser = |s: &str| {
            s.parse::<usize>()
                .map(|n| track_value(n, &MAX_FILES_WAS_SET))
                .map_err(|e| e.to_string())
        }
    )]
    pub max_files: usize,

    /// Maximum number of diff lines to include per file
    ///
    /// Limits the size of diffs to prevent excessive context. If a diff is
    /// larger than this, it will be truncated with a note.
    ///
    /// Example:
    ///   --max-diff-lines 100
    #[arg(
        short = 'l',
        long,
        default_value = "50",
        value_name = "LINES",
        help_heading = "Diff Options",
        value_parser = |s: &str| {
            s.parse::<usize>()
                .map(|n| track_value(n, &MAX_DIFF_LINES_WAS_SET))
                .map_err(|e| e.to_string())
        }
    )]
    pub max_diff_lines: usize,

    /// Enable interactive confirmation before committing
    ///
    /// By default, the tool will commit without confirmation. Use this flag to
    /// review the generated commit message before committing.
    ///
    /// Example:
    ///   --confirm  # Ask for confirmation before committing
    #[arg(
        long = "confirm",
        help = "Ask for confirmation before committing",
        help_heading = "Commit Options",
        default_value_t = true,
        action = clap::ArgAction::SetFalse
    )]
    pub no_confirm: bool,

    /// Path to a custom prompt template file
    ///
    /// The template should be a text file used to generate the prompt sent to the AI.
    /// Custom templates receive the repo analysis context through the prompt builder.
    ///
    /// Example:
    ///   --template ./my-custom-prompt.txt
    #[arg(long, value_name = "FILE", help_heading = "Customization")]
    pub template: Option<PathBuf>,

    /// Additional user context to include in the commit prompt
    ///
    /// Use this to tell the model why the change was made or what to emphasize.
    ///
    /// Example:
    ///   --context "focus on cleanup before release"
    #[arg(long, value_name = "TEXT", help_heading = "Customization")]
    pub context: Option<String>,

    /// Generate a pull request title and body instead of a commit message
    ///
    /// In dry-run mode this prints a PR draft to stdout.
    #[arg(long, help_heading = "Commit Options")]
    pub pr: bool,

    /// Push the current branch to origin after creating the commit
    ///
    /// This uses `git push --set-upstream origin <current-branch>`.
    #[arg(long, help_heading = "Commit Options")]
    pub push: bool,

    /// Create a commit, push the branch, and open a pull request in one flow
    ///
    /// This combines the existing commit, push, and PR paths into one command.
    #[arg(long = "push-pr", help_heading = "Commit Options")]
    pub push_pr: bool,

    /// Show the git analysis and generated commit message without committing
    ///
    /// This is useful for previewing what the commit would look like.
    /// Combine with --verbose to see the full prompt sent to the AI.
    ///
    /// Example:
    ///   --dry-run
    #[arg(short = 'd', long, help_heading = "Debug Options")]
    pub dry_run: bool,

    /// Enable verbose output for debugging
    ///
    /// Shows additional information about what the tool is doing,
    /// including the full prompt sent to the AI model.
    ///
    /// Example:
    ///   --verbose
    ///   -v
    #[arg(short, long, help_heading = "Debug Options")]
    pub verbose: bool,

    /// Port for the local provider server
    ///
    /// Change this if your local provider is running on a non-default port.
    /// This is primarily used for local Ollama and local OpenAI-compatible endpoints.
    ///
    /// Default: 11434
    #[arg(
        short = 'p',
        long,
        default_value = "11434",
        value_name = "PORT",
        value_parser = |s: &str| {
            s.parse::<u16>()
                .map(|n| track_value(n, &PORT_WAS_SET))
                .map_err(|e| e.to_string())
        }
    )]
    pub port: u16,

    /// Timeout for AI generation in seconds
    ///
    /// Default: 60 seconds
    #[arg(
        short = 't',
        long,
        default_value = "60",
        value_name = "SECONDS",
        help_heading = "Advanced",
        value_parser = |s: &str| {
            s.parse::<u64>()
                .map(|n| track_value(n, &TIMEOUT_WAS_SET))
                .map_err(|e| e.to_string())
        }
    )]
    pub timeout_seconds: u64,

    /// Automatically stage all unstaged changes before generation
    ///
    /// This stages unstaged changes before commit or PR generation.
    ///
    /// Example:
    ///   --add-unstaged
    #[arg(short = 'a', long, help_heading = "Staging Options")]
    pub add_unstaged: bool,

    /// List all available models for the selected provider and exit
    ///
    /// This connects to the selected provider backend and prints its visible models.
    /// The tool exits after displaying the list.
    ///
    /// Example:
    ///   --list-models
    #[arg(long, help_heading = "Model Options")]
    pub list_models: bool,
}

impl Args {
    fn normalize_provider_defaults(mut args: Self) -> Self {
        if args.provider.trim() != "ollama" && !MODEL_WAS_SET.with(|f| f.load(Ordering::Relaxed)) {
            args.model.clear();
        }

        args
    }

    pub fn try_parse_from<I, T>(itr: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        reset_tracked_flags();
        let raw_args: Vec<OsString> = itr.into_iter().map(Into::into).collect();
        let parsed = <Self as Parser>::try_parse_from(raw_args.clone())?;
        sync_tracked_flags_from_args(&raw_args);
        Ok(Self::normalize_provider_defaults(parsed))
    }

    pub fn parse() -> Self {
        reset_tracked_flags();
        let raw_args: Vec<OsString> = std::env::args_os().collect();
        let parsed = <Self as Parser>::parse_from(raw_args.clone());
        sync_tracked_flags_from_args(&raw_args);
        Self::normalize_provider_defaults(parsed)
    }

    /// Load configuration from the default location and override with command-line arguments
    pub fn load() -> Self {
        // First, parse command line arguments to see which ones were explicitly set
        let mut args = Self::parse();

        // Then load the config file
        if let Ok(config) = Config::load() {
            println!("Using model from config: {}", config.model);

            // Only override values that weren't explicitly set via command line
            if !PROVIDER_WAS_SET.with(|f| f.load(Ordering::Relaxed)) {
                args.provider = config.provider;
            }

            if !MODEL_WAS_SET.with(|f| f.load(Ordering::Relaxed)) {
                args.model = config.model;
            }

            if args.base_url.is_none() {
                args.base_url = config.base_url;
            }

            if args.api_key.is_none() {
                args.api_key = config.api_key;
            }

            if !MAX_FILES_WAS_SET.with(|f| f.load(Ordering::Relaxed)) {
                args.max_files = config.max_files;
            }

            if !MAX_DIFF_LINES_WAS_SET.with(|f| f.load(Ordering::Relaxed)) {
                args.max_diff_lines = config.max_diff_lines;
            }

            if !PORT_WAS_SET.with(|f| f.load(Ordering::Relaxed)) {
                args.port = config.port;
            }

            if !TIMEOUT_WAS_SET.with(|f| f.load(Ordering::Relaxed)) {
                args.timeout_seconds = config.timeout_seconds;
            }
        }

        args
    }
}
