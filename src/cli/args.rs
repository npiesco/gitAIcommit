use clap::Parser;
use std::path::PathBuf;
use std::sync::OnceLock;
use crate::config::Config;
use crate::ollama::client::OllamaClient;
use crate::ollama::OllamaClientTrait;
use tokio::runtime::Runtime;
use tokio;
use std::sync::atomic::{AtomicBool, Ordering};

// Track which fields were explicitly set via command line
thread_local! {
    static MODEL_WAS_SET: AtomicBool = AtomicBool::new(false);
    static MAX_FILES_WAS_SET: AtomicBool = AtomicBool::new(false);
    static MAX_DIFF_LINES_WAS_SET: AtomicBool = AtomicBool::new(false);
    static PORT_WAS_SET: AtomicBool = AtomicBool::new(false);
    static TIMEOUT_WAS_SET: AtomicBool = AtomicBool::new(false);
}

// Helper function to track when a value is set
fn track_value<T>(value: T, flag: &'static std::thread::LocalKey<AtomicBool>) -> T {
    flag.with(|f| f.store(true, Ordering::Relaxed));
    value
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
                    client.get_last_model().await.unwrap_or_else(|_| None)
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
                client.get_last_model().await.unwrap_or_else(|_| None)
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
    "gemma3:4b".to_string()
}

/// Command-line arguments for git-ai-commit
/// 
/// This tool generates AI-powered commit messages by analyzing your git changes.
/// It uses an embedded Ollama instance to generate meaningful commit messages.
#[derive(Parser, Debug)]
#[command(
    name = "git-ai-commit",
    about = "Generate AI-powered commit messages using embedded Ollama",
    long_about = "\
Generate meaningful commit messages by analyzing your git changes with AI.\n\n\
USAGE EXAMPLES:\n\n\
  # Basic usage (staged changes only)\n  $ git-ai-commit\n\n\
  # Stage all changes before committing\n  $ git-ai-commit --add-unstaged\n\n\
  # Preview changes without committing\n  $ git-ai-commit --dry-run\n\n\
  # Use a specific AI model\n  $ git-ai-commit --model llama3\n\n\
  # Show verbose output for debugging\n  $ git-ai-commit --verbose\n\n\
  # Use a custom prompt template\n  $ git-ai-commit --template ./my-prompt.txt\n\n\
  # Increase diff context for better messages\n  $ git-ai-commit --max-files 20 --max-diff-lines 100\n\n\
  # Run with custom Ollama port\n  $ git-ai-commit --port 12345\n\n\
For more information on each option, use --help.",
    version,
    propagate_version = true
)]
pub struct Args {
    /// AI model to use for commit message generation
    /// 
    /// If not specified, the tool will use the value from the config file,
    /// or fall back to the last model used with Ollama, or 'gemma3:4b' as the final fallback.
    /// 
    /// Examples:
    ///   --model llama3
    ///   -m mistral
    #[arg(
        short, 
        long, 
        default_value_t = get_default_model(),
        value_name = "MODEL",
        value_parser = |s: &str| {
            let s = s.to_string();
            Ok::<_, std::convert::Infallible>(track_value(s, &MODEL_WAS_SET))
        }
    )]
    pub model: String,
    
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
        help = "Ask for confirmation before committing (default: false)",
        help_heading = "Commit Options",
        default_value_t = true,
        action = clap::ArgAction::SetFalse
    )]
    pub no_confirm: bool,
    
    /// Path to a custom prompt template file
    /// 
    /// The template should be a text file that will be used to generate the
    /// prompt sent to the AI. Use placeholders like {diff} and {status}.
    /// 
    /// Example:
    ///   --template ./my-custom-prompt.txt
    #[arg(
        long, 
        value_name = "FILE",
        help_heading = "Customization"
    )]
    pub template: Option<PathBuf>,
    
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
    
    /// Custom port for the Ollama server
    /// 
    /// Change this if you're running Ollama on a non-default port.
    /// Only affects the embedded Ollama instance.
    /// Port for the Ollama server
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
    
    /// Automatically stage all unstaged changes before generating commit message
    /// 
    /// This is equivalent to running 'git add .' before generating the commit.
    /// The tool will show you what changes will be staged before proceeding.
    /// 
    /// Example:
    ///   --add-unstaged
    #[arg(
        short = 'a',
        long,
        help_heading = "Staging Options"
    )]
    pub add_unstaged: bool,
    
    /// List all available Ollama models and exit
    /// 
    /// This will connect to the Ollama server and list all locally available models.
    /// The tool will exit after displaying the list.
    /// 
    /// Example:
    ///   --list-models
    #[arg(
        long,
        help_heading = "Model Options"
    )]
    pub list_models: bool,
}

impl Args {
    /// Load configuration from the default location and override with command-line arguments
    pub fn load() -> Self {
        // First, parse command line arguments to see which ones were explicitly set
        let mut args = Self::parse();
        
        // Then load the config file
        if let Ok(config) = Config::load() {
            println!("Using model from config: {}", config.model);
            
            // Only override values that weren't explicitly set via command line
            if !MODEL_WAS_SET.with(|f| f.load(Ordering::Relaxed)) {
                args.model = config.model;
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
