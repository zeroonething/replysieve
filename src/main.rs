use anyhow::{Context, bail};
use clap::{Args, Parser, Subcommand};
use replysieve::{API_KEY_ENV, Checker, Comment, Config, Level, Options, Preset};
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

fn api_key() -> Option<String> {
    std::env::var(API_KEY_ENV).ok()
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
    let checker = Checker::new(options(&args.common)?, api_key());
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
    for c in &comments {
        let verdict = checker.check(c, &recent).await?;
        println!("{}", serde_json::to_string(&verdict)?);
    }
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
