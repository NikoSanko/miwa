use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(name = "miwa", version, about = "doc / req / test drift detection")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Initialize `.miwa/config.yaml` in the current repository.
    Init,
    /// Detect drift between the current graph and the lock.
    Check {
        #[arg(long)]
        gate: bool,
        #[arg(long)]
        diff: bool,
        #[arg(long)]
        base: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Record the current state of edges into the lock.
    Reconcile {
        #[arg(long = "edge")]
        edge: Vec<String>,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        reason: Option<String>,
    },
    /// Semantically judge drifted / new edges.
    Judge {
        #[arg(long = "edge")]
        edge: Vec<String>,
        #[arg(long)]
        auto_reconcile: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Init => run_init(),
        Command::Check {
            gate,
            diff,
            base,
            format,
        } => run_check(gate, diff, base, format),
        Command::Reconcile { edge, all, reason } => run_reconcile(edge, all, reason),
        Command::Judge {
            edge,
            auto_reconcile,
        } => run_judge(edge, auto_reconcile),
    }
}

fn run_init() {
    todo!()
}

fn run_check(gate: bool, diff: bool, base: Option<String>, format: OutputFormat) {
    let _ = (gate, diff, base, format);
    todo!()
}

fn run_reconcile(edge: Vec<String>, all: bool, reason: Option<String>) {
    let _ = (edge, all, reason);
    todo!()
}

fn run_judge(edge: Vec<String>, auto_reconcile: bool) {
    let _ = (edge, auto_reconcile);
    todo!()
}
