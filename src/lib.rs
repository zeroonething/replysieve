//! replysieve judges user comments: approve, reject or needs_review, with a
//! reason, at light, medium or heavy strictness.
//!
//! Free pre-checks (length, stock phrases, duplicates, injection markers) run
//! first with no network call. Comments that pass go to a decision model on
//! OpenRouter, tried model by model in the order you give.
//!
//! ```
//! use replysieve::{Checker, Comment, Decision, Level, Options, Reason};
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), replysieve::Error> {
//! let checker = Checker::new(
//!     Options { level: Level::Medium, ..Options::default() },
//!     std::env::var("OPENROUTER_API_KEY").ok(),
//! );
//!
//! // Stock praise is rejected by a free pre-check, with no model call.
//! let verdict = checker.check(&Comment::new("Great product, love it!!"), &[]).await?;
//! assert_eq!(verdict.verdict, Decision::Reject);
//! assert_eq!(verdict.reason, Reason::StockPhrase);
//! assert_eq!(verdict.model, None);
//!
//! // Anything that passes the pre-checks goes to the model.
//! let comment = Comment {
//!     title: Some("Taskly, a todo app with offline sync".into()),
//!     ..Comment::new("Does offline sync merge edits made on two phones?")
//! };
//! match checker.check(&comment, &[]).await {
//!     Ok(v) => println!("{}", serde_json::to_string(&v).unwrap()),
//!     Err(e) => eprintln!("{e}"), // no OPENROUTER_API_KEY set
//! }
//! # Ok(())
//! # }
//! ```
#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod config;
mod judge;
mod openrouter;
mod precheck;
mod presets;
mod types;

pub use config::Config;
pub use openrouter::DEFAULT_BASE_URL;
pub use precheck::normalize;
pub use presets::Preset;
pub use types::{Comment, Decision, Level, Reason, Verdict};

use precheck::Prechecks;
use std::time::Instant;

/// The default OpenRouter decision model.
pub const DEFAULT_MODEL: &str = "~typesafe/jev-latest";

/// Comments judged at once by [`Checker::check_batch`].
const BATCH_CONCURRENCY: usize = 8;

/// Environment variable holding the OpenRouter API key.
pub const API_KEY_ENV: &str = "OPENROUTER_API_KEY";

/// How to judge comments.
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    /// Strictness level.
    pub level: Level,
    /// Built-in preset.
    pub preset: Preset,
    /// Plain-English rules enforced at every level.
    pub rules: Vec<String>,
    /// Extra stock phrases for the free pre-check.
    pub stock_phrases: Vec<String>,
    /// OpenRouter decision models, tried in order.
    pub models: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            level: Level::default(),
            preset: Preset::default(),
            rules: Vec::new(),
            stock_phrases: Vec::new(),
            models: vec![DEFAULT_MODEL.to_string()],
        }
    }
}

/// Errors that stop a check before a verdict exists.
#[derive(Debug)]
pub enum Error {
    /// The comment needs a model call and no API key was given.
    MissingApiKey,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::MissingApiKey => write!(
                f,
                "{API_KEY_ENV} is not set; get a key at https://openrouter.ai/keys and export it"
            ),
        }
    }
}

impl std::error::Error for Error {}

fn normalize_all(texts: &[String]) -> Vec<String> {
    texts.iter().map(|t| normalize(t)).collect()
}

/// Judges comments with one set of options.
#[derive(Debug, Clone)]
pub struct Checker {
    options: Options,
    prechecks: Prechecks,
    client: Option<openrouter::Client>,
    questions: serde_json::Value,
}

impl Checker {
    /// A checker using `api_key` for model calls. Pass `None` to allow only
    /// pre-check verdicts; a comment that needs the model then fails with
    /// [`Error::MissingApiKey`].
    pub fn new(options: Options, api_key: Option<String>) -> Self {
        Self::with_base_url(options, api_key, DEFAULT_BASE_URL)
    }

    /// Like [`Checker::new`], against another OpenRouter-compatible base URL.
    pub fn with_base_url(options: Options, api_key: Option<String>, base_url: &str) -> Self {
        Checker {
            prechecks: Prechecks::new(Self::extra_stock(&options)),
            client: api_key
                .filter(|k| !k.trim().is_empty())
                .map(|k| openrouter::Client::new(base_url, k)),
            questions: judge::questions(options.preset, &options.rules),
            options,
        }
    }

    /// The same checker at another strictness level.
    pub fn at_level(mut self, level: Level) -> Self {
        self.options.level = level;
        self
    }

    /// A checker with other options that shares this one's HTTP client and key.
    pub fn with_options(&self, options: Options) -> Self {
        Checker {
            prechecks: Prechecks::new(Self::extra_stock(&options)),
            questions: judge::questions(options.preset, &options.rules),
            client: self.client.clone(),
            options,
        }
    }

    fn extra_stock(options: &Options) -> Vec<&str> {
        options
            .preset
            .stock_phrases()
            .iter()
            .copied()
            .chain(options.stock_phrases.iter().map(String::as_str))
            .collect()
    }

    /// The options this checker judges with.
    pub fn options(&self) -> &Options {
        &self.options
    }

    /// Run only the free pre-checks. `recent` holds texts recently posted by
    /// the same author or thread; an identical comment is a duplicate.
    pub fn precheck(&self, comment: &Comment, recent: &[String]) -> Option<Verdict> {
        self.precheck_normalized(comment, &normalize_all(recent))
    }

    /// [`Checker::precheck`] with `recent` already normalized.
    fn precheck_normalized(&self, comment: &Comment, recent: &[String]) -> Option<Verdict> {
        let start = Instant::now();
        let reason = self
            .prechecks
            .run(&comment.text, self.options.level, recent)?;
        Some(Verdict {
            verdict: Decision::Reject,
            reason,
            confidence: 1.0,
            level: self.options.level,
            model: None,
            latency_ms: start.elapsed().as_millis() as u64,
            rule: None,
        })
    }

    /// Judge many comments, a few at a time. Verdicts come back in input order.
    pub async fn check_batch(
        &self,
        comments: &[Comment],
        recent: &[String],
    ) -> Result<Vec<Verdict>, Error> {
        use futures_util::{StreamExt, stream};
        // Collected first: a stream holding the mapping closure is not provably
        // Send to rustc, and axum handlers and MCP tools need it to be.
        // Normalized once for the whole batch, not once per comment.
        let recent = normalize_all(recent);
        let checks: Vec<_> = comments
            .iter()
            .map(|c| self.check_normalized(c, &recent))
            .collect();
        stream::iter(checks)
            .buffered(BATCH_CONCURRENCY)
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect()
    }

    /// Judge one comment: free pre-checks first, then the models in order.
    /// When no model answers, the verdict is needs_review with reason
    /// model_unavailable, never a silent approve.
    pub async fn check(&self, comment: &Comment, recent: &[String]) -> Result<Verdict, Error> {
        self.check_normalized(comment, &normalize_all(recent)).await
    }

    /// [`Checker::check`] with `recent` already normalized.
    async fn check_normalized(
        &self,
        comment: &Comment,
        recent: &[String],
    ) -> Result<Verdict, Error> {
        let start = Instant::now();
        if let Some(v) = self.precheck_normalized(comment, recent) {
            return Ok(v);
        }
        let client = self.client.as_ref().ok_or(Error::MissingApiKey)?;
        let state = judge::state(comment, self.options.preset);
        let level = self.options.level;
        for model in &self.options.models {
            let asked = Instant::now();
            let answer = match client.decide(model, &state, &self.questions).await {
                Ok(resp) => judge::judge(level, &self.options.rules, &resp.answers)
                    .map(|j| (resp.model, j))
                    .ok_or_else(|| "malformed answers".to_string()),
                Err(e) => Err(e),
            };
            match answer {
                Ok((answered_by, j)) => {
                    log::info!(
                        "model {model} answered in {} ms",
                        asked.elapsed().as_millis()
                    );
                    return Ok(Verdict {
                        verdict: j.decision,
                        reason: j.reason,
                        confidence: j.confidence,
                        level,
                        model: Some(answered_by),
                        latency_ms: start.elapsed().as_millis() as u64,
                        rule: j.rule,
                    });
                }
                Err(e) => log::warn!("model {model} failed ({e}); trying the next model"),
            }
        }
        log::warn!("no model answered");
        Ok(Verdict {
            verdict: Decision::NeedsReview,
            reason: Reason::ModelUnavailable,
            confidence: 0.0,
            level,
            model: None,
            latency_ms: start.elapsed().as_millis() as u64,
            rule: None,
        })
    }
}
