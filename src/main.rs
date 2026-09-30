use clap::{Parser, Subcommand, ValueEnum};
use miwa::source::Source;

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
        #[arg(long = "edge", conflicts_with = "all")]
        edge: Vec<String>,
        #[arg(long, conflicts_with = "edge")]
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

fn load_config(root: &std::path::Path) -> anyhow::Result<miwa::config::Config> {
    let path = root.join(miwa::config::CONFIG_PATH);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("failed to read {}: {e}", miwa::config::CONFIG_PATH))?;
    miwa::config::Config::parse(&text)
}

fn evaluate(
    source: &dyn Source,
    config: &miwa::config::Config,
    lock: &miwa::lock::Lock,
) -> anyhow::Result<Vec<miwa::check::Finding>> {
    let graph = miwa::graph::build_graph(source, config)?;
    Ok(miwa::check::check(&graph, lock))
}

fn run_init() {
    match run_init_inner() {
        Ok(()) => std::process::exit(0),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

fn run_init_inner() -> anyhow::Result<()> {
    let root = std::env::current_dir()?;
    let config_path = root.join(miwa::config::CONFIG_PATH);
    if config_path.exists() {
        anyhow::bail!("{} already exists", miwa::config::CONFIG_PATH);
    }
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let config = miwa::config::Config::default_config();
    std::fs::write(&config_path, config.to_yaml())?;
    Ok(())
}

fn run_check(gate: bool, diff: bool, base: Option<String>, format: OutputFormat) {
    match run_check_inner(gate, diff, base, format) {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

fn run_check_inner(
    gate: bool,
    diff: bool,
    base: Option<String>,
    format: OutputFormat,
) -> anyhow::Result<i32> {
    let root = std::env::current_dir()?;
    let config = load_config(&root)?;
    let source = miwa::source::FsSource::new(root.clone());
    let lock = miwa::lock::Lock::load(&root)?;
    let mut findings = evaluate(&source, &config, &lock)?;

    if diff {
        let base_ref = base.unwrap_or_else(|| config.diff_base.clone());
        let merge_base_rev = miwa::source::merge_base(&root, &base_ref)?;
        let base_source = miwa::source::GitTreeSource::new(&root, &merge_base_rev)?;
        // `.miwa/` is excluded from `list_files`, so probe for the config file
        // directly instead of checking the file listing.
        let base_findings = match base_source.read(miwa::config::CONFIG_PATH) {
            Ok(text_bytes) => {
                let text = String::from_utf8(text_bytes)?;
                let base_config = miwa::config::Config::parse(&text)?;
                let base_lock = miwa::lock::Lock::load_from_source(&base_source)?;
                evaluate(&base_source, &base_config, &base_lock)?
            }
            Err(e) if miwa::source::is_not_found(&e) => Vec::new(),
            Err(e) => return Err(e),
        };
        findings = miwa::check::diff_findings(&findings, &base_findings);
    }

    match format {
        OutputFormat::Text => print!("{}", miwa::output::render_text(&findings)),
        OutputFormat::Json => println!("{}", miwa::output::render_json(&findings)),
    }

    Ok(miwa::check::exit_code(&findings, gate))
}

fn run_reconcile(edge: Vec<String>, all: bool, reason: Option<String>) {
    match run_reconcile_inner(edge, all, reason) {
        Ok(()) => std::process::exit(0),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

fn run_reconcile_inner(edge: Vec<String>, all: bool, reason: Option<String>) -> anyhow::Result<()> {
    if !all && edge.is_empty() {
        anyhow::bail!("usage: miwa reconcile --edge <id> [--edge <id> ...] | --all");
    }
    let root = std::env::current_dir()?;
    let config = load_config(&root)?;
    let source = miwa::source::FsSource::new(root.clone());
    let graph = miwa::graph::build_graph(&source, &config)?;
    let lock = miwa::lock::Lock::load(&root)?;

    let target = if all {
        miwa::reconcile::ReconcileTarget::All
    } else {
        miwa::reconcile::ReconcileTarget::Edges(edge)
    };

    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let new_lock = miwa::reconcile::reconcile(&graph, &lock, target, reason, &now)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    new_lock.save(&root)?;
    Ok(())
}

fn run_judge(edge: Vec<String>, auto_reconcile: bool) {
    let _ = (edge, auto_reconcile);
    eprintln!("judge is not implemented yet (Phase B)");
    std::process::exit(1);
}
