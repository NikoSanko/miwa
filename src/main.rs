use anyhow::Context;
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
    /// Create or update `index.md` files from the documents they list.
    Index,
    /// Write a static HTML site for reading the documents.
    Site {
        #[arg(long, default_value = "_site")]
        out: String,
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
        Command::Index => run_index(),
        Command::Site { out } => run_site(out),
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
                let base_config = miwa::config::Config::parse(&text).with_context(|| {
                    format!("invalid .miwa/config.yaml at merge-base {merge_base_rev}")
                })?;
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

fn run_index() {
    let result = std::env::current_dir()
        .map_err(anyhow::Error::from)
        .and_then(|root| {
            let config = load_config(&root)?;
            miwa::index::run(&root, &config)
        });
    match result {
        Ok(results) => {
            for (status, path) in results {
                println!("{} {path}", status.as_str());
            }
            std::process::exit(0)
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

fn run_site(out: String) {
    let result = std::env::current_dir()
        .map_err(anyhow::Error::from)
        .and_then(|root| {
            let config = load_config(&root)?;
            miwa::site::run(&root, &config, std::path::Path::new(&out))
        });
    match result {
        Ok(warnings) => {
            for w in warnings {
                eprintln!("{w}");
            }
            std::process::exit(0)
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
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
    match run_judge_inner(edge, auto_reconcile) {
        Ok(()) => std::process::exit(0),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

fn jev_http(
    agent: &ureq::Agent,
    endpoint: &str,
    key: &str,
    value: &serde_json::Value,
) -> Result<String, miwa::judge::JudgeError> {
    use miwa::judge::JudgeError;
    let redact = |s: String| s.replace(key, "***");
    let response = agent
        .post(endpoint)
        .header("Authorization", format!("Bearer {key}"))
        .header("Content-Type", "application/json")
        .send_json(value)
        .map_err(|e| match e {
            ureq::Error::StatusCode(status) => JudgeError::HttpStatus { status },
            other => JudgeError::Transport {
                message: redact(other.to_string()),
            },
        })?;
    response
        .into_body()
        .read_to_string()
        .map_err(|e| JudgeError::Transport {
            message: redact(e.to_string()),
        })
}

fn run_judge_inner(edge: Vec<String>, auto_reconcile: bool) -> anyhow::Result<()> {
    use miwa::judge;

    let root = std::env::current_dir()?;
    let config = load_config(&root)?;
    let key = std::env::var(judge::JEV_API_KEY_ENV).unwrap_or_default();
    if key.is_empty() {
        anyhow::bail!(
            "{}",
            judge::JudgeError::MissingApiKey(judge::JEV_API_KEY_ENV.to_string())
        );
    }

    let source = miwa::source::FsSource::new(root.clone());
    let graph = miwa::graph::build_graph(&source, &config)?;
    let lock = miwa::lock::Lock::load(&root)?;
    let findings = miwa::check::check(&graph, &lock);
    let inputs = judge::select_targets(&graph, &findings, &edge)?;

    if inputs.is_empty() {
        print!("{}", judge::render_text(&[]));
        return Ok(());
    }

    eprintln!("note: sending doc/req/test contents and descriptions to api.typesafe.ai");

    let endpoint = std::env::var(judge::JEV_ENDPOINT_OVERRIDE_ENV)
        .unwrap_or_else(|_| judge::JEV_ENDPOINT.to_string());
    let http: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(std::time::Duration::from_secs(10)))
        .timeout_global(Some(std::time::Duration::from_secs(60)))
        .build()
        .into();
    let call = |value: &serde_json::Value| jev_http(&http, &endpoint, &key, value);
    let judgments = judge::judge_edges(
        &inputs,
        config.judge.threshold,
        config.judge.max_body_bytes,
        &call,
    )?;
    print!("{}", judge::render_text(&judgments));

    if auto_reconcile {
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        judge::apply_auto_reconcile(&lock, &inputs, &judgments, &now).save(&root)?;
    }
    Ok(())
}
