use anyhow::{Context, bail};
use clap::{Args, Parser, Subcommand};
use futures_util::{StreamExt, stream};
use replysieve::{API_KEY_ENV, Checker, Comment, Config, DEFAULT_BASE_URL, Level, Options, Preset};
use std::io::BufRead;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "replysieve",
    version,
    about = "Judge comments: approve, reject or needs_review, with a reason."
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Judge one comment, or a JSONL batch with --file.
    Check(CheckArgs),
}

#[derive(Args)]
struct Common {
    /// Strictness: light, medium or heavy [default: medium].
    #[arg(long)]
    level: Option<Level>,
    /// Built-in preset: comments, product-review or launch-thread [default: comments].
    #[arg(long)]
    preset: Option<String>,
    /// TOML config file with level, preset, models, rules and stock_phrases.
    #[arg(long)]
    config: Option<PathBuf>,
    /// OpenRouter decision model; repeat for fallback order. Overrides the config.
    #[arg(long = "model")]
    models: Vec<String>,
    /// Print timing and model-fallback events to stderr. Never prints comment text.
    #[arg(long, short)]
    verbose: bool,
}

#[derive(Args)]
struct CheckArgs {
    /// The comment text.
    #[arg(required_unless_present = "file", conflicts_with = "file")]
    text: Option<String>,
    /// JSONL file: one {"text", "title"?, "description"?, "parent"?} object per line.
    #[arg(long)]
    file: Option<PathBuf>,
    /// Title of the post or product the comment is on.
    #[arg(long)]
    title: Option<String>,
    /// Description of the post or product.
    #[arg(long)]
    description: Option<String>,
    /// The comment this one replies to.
    #[arg(long)]
    parent: Option<String>,
    /// File of recent texts, one per line; an identical comment is a duplicate.
    #[arg(long)]
    recent: Option<PathBuf>,
    #[command(flatten)]
    common: Common,
}

fn options(common: &Common) -> anyhow::Result<Options> {
    let config = match &common.config {
        Some(path) => Config::load(path)?,
        None => Config::default(),
    };
    let preset_name = common.preset.as_deref().or(config.preset.as_deref());
    let preset = match preset_name {
        Some(name) => name.parse::<Preset>().map_err(anyhow::Error::msg)?,
        None => Preset::default(),
    };
    let mut opts = Options {
        level: common.level.or(config.level).unwrap_or_default(),
        preset,
        rules: config.rules,
        stock_phrases: config.stock_phrases,
        ..Options::default()
    };
    if !common.models.is_empty() {
        opts.models = common.models.clone();
    } else if !config.models.is_empty() {
        opts.models = config.models;
    }
    Ok(opts)
}

/// Environment variable that points the client at another base URL (tests, proxies).
const BASE_URL_ENV: &str = "REPLYSIEVE_OPENROUTER_URL";

/// Comments judged at once in a batch.
const CONCURRENCY: usize = 8;

fn checker(common: &Common) -> anyhow::Result<Checker> {
    let base = std::env::var(BASE_URL_ENV).unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
    Ok(Checker::with_base_url(
        options(common)?,
        std::env::var(API_KEY_ENV).ok(),
        &base,
    ))
}

/// Stderr logger for --verbose. Messages carry timing and model names only.
struct StderrLog;

impl log::Log for StderrLog {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.target().starts_with("replysieve")
    }
    fn log(&self, r: &log::Record) {
        if self.enabled(r.metadata()) {
            eprintln!("replysieve: {}", r.args());
        }
    }
    fn flush(&self) {}
}

fn init_log(verbose: bool) {
    if verbose && log::set_logger(&StderrLog).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
}

fn read_lines(path: &PathBuf) -> anyhow::Result<Vec<String>> {
    let file =
        std::fs::File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    std::io::BufReader::new(file)
        .lines()
        .collect::<Result<_, _>>()
        .with_context(|| format!("cannot read {}", path.display()))
}

async fn check(args: CheckArgs) -> anyhow::Result<()> {
    init_log(args.common.verbose);
    let checker = checker(&args.common)?;
    let recent = match &args.recent {
        Some(path) => read_lines(path)?
            .into_iter()
            .filter(|l| !l.trim().is_empty())
            .collect(),
        None => Vec::new(),
    };
    let comments = match (&args.text, &args.file) {
        (Some(text), _) => vec![Comment {
            text: text.clone(),
            title: args.title.clone(),
            description: args.description.clone(),
            parent: args.parent.clone(),
        }],
        (None, Some(path)) => {
            let mut out = Vec::new();
            for (i, line) in read_lines(path)?.iter().enumerate() {
                if line.trim().is_empty() {
                    continue;
                }
                let c: Comment = serde_json::from_str(line).with_context(|| {
                    format!("{}: line {} is not a comment object", path.display(), i + 1)
                })?;
                out.push(c);
            }
            out
        }
        (None, None) => bail!("give a comment text or --file"),
    };
    let started = std::time::Instant::now();
    let mut verdicts = stream::iter(&comments)
        .map(|c| checker.check(c, &recent))
        .buffered(CONCURRENCY);
    while let Some(verdict) = verdicts.next().await {
        println!("{}", serde_json::to_string(&verdict?)?);
    }
    log::info!(
        "judged {} comment(s) in {} ms",
        comments.len(),
        started.elapsed().as_millis()
    );
    Ok(())
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.cmd {
        Cmd::Check(args) => check(args).await,
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}
