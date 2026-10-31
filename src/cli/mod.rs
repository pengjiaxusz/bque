use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "bque",
    about = "Universal build queue, CPU governor, and clean-room benchmark protector for Windows",
    version,
    long_about = "bque serializes builds and heavy tasks across multiple repositories, adjusts process priorities, throttles CPU usage via Job Objects, and guarantees clean, undisturbed CPU conditions for benchmarks."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Execute a command under global queue lock with priority and CPU budget limits
    Run(RunArgs),

    /// Execute a benchmark under exclusive lock, automatically freezing background builds
    Bench(BenchArgs),

    /// Show current lock holder, queue waiters, observers, and system budget status
    Status(StatusArgs),

    /// Manually pause all background builds (e.g. for gaming or interactive profiling)
    Pause(PauseArgs),

    /// Resume paused background builds
    Resume,

    /// Clean dead locks, orphaned waiter records, and zombie state files
    Clean,
}

#[derive(Args, Debug)]
pub struct RunArgs {
    /// The command and arguments to execute
    #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
    pub command: Vec<String>,

    /// Human-readable task description (inferred from command if omitted)
    #[arg(short = 'd', long)]
    pub desc: Option<String>,

    /// Process priority class
    #[arg(short = 'p', long, value_enum, default_value = "idle")]
    pub priority: PriorityArg,

    /// CPU rate limit percentage (1-100) enforced via Windows Job Object soft-cap
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..=100))]
    pub cpu_rate: Option<u32>,

    /// Enforce hard CPU rate limit (never exceeds budget, even when system is idle)
    #[arg(long, default_value_t = false)]
    pub hard_cap: bool,

    /// Maximum queue wait timeout in seconds (0 = wait indefinitely)
    #[arg(short = 't', long, default_value_t = 7200)]
    pub timeout: u64,

    /// CPU affinity cores (e.g. '0,1,2,3', '0-3', 'e-cores', or 'p-cores')
    #[arg(long)]
    pub cores: Option<String>,

    /// Observer mode: register in status and apply limits, but do not acquire mutual exclusion
    #[arg(long, default_value_t = false)]
    pub observer: bool,

    /// Disable dynamic background CPU governor (keep fixed initial budget)
    #[arg(long, default_value_t = false)]
    pub no_governor: bool,

    /// Working directory for command execution (defaults to current working directory)
    #[arg(long)]
    pub cwd: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct BenchArgs {
    /// The benchmark command and arguments to execute
    #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
    pub command: Vec<String>,

    /// Description of the benchmark run
    #[arg(short = 'd', long)]
    pub desc: Option<String>,

    /// Process priority class for benchmark execution
    #[arg(short = 'p', long, value_enum, default_value = "high")]
    pub priority: PriorityArg,

    /// CPU affinity cores (e.g. 'p-cores', '0,1,2,3') to eliminate scheduling jitter
    #[arg(long)]
    pub cores: Option<String>,

    /// Automatically freeze (suspend) running background builds while benchmark executes
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    pub freeze_builds: bool,

    /// Maximum benchmark run duration in seconds
    #[arg(short = 't', long, default_value_t = 3600)]
    pub timeout: u64,

    /// Working directory for benchmark execution
    #[arg(long)]
    pub cwd: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct StatusArgs {
    /// Output queue state as machine-readable JSON
    #[arg(long, default_value_t = false)]
    pub json: bool,

    /// Continuously refresh and display queue status in real time
    #[arg(short = 'w', long, default_value_t = false)]
    pub watch: bool,
}

#[derive(Args, Debug)]
pub struct PauseArgs {
    /// Optional reason for manual pause
    #[arg(short = 'r', long, default_value = "Manual user pause")]
    pub reason: String,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum PriorityArg {
    Idle,
    BelowNormal,
    Normal,
    High,
}

impl std::fmt::Display for PriorityArg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PriorityArg::Idle => write!(f, "Idle"),
            PriorityArg::BelowNormal => write!(f, "BelowNormal"),
            PriorityArg::Normal => write!(f, "Normal"),
            PriorityArg::High => write!(f, "High"),
        }
    }
}
