//! replysieve judges user comments: approve, reject or needs_review, with a
//! reason, at light, medium or heavy strictness.
//!
//! Free pre-checks (length, stock phrases, duplicates, injection markers) run
//! first with no network call. Comments that pass go to a decision model on
//! OpenRouter.
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
        let extra: Vec<&str> = options
            .preset
            .stock_phrases()
            .iter()
            .copied()
            .chain(options.stock_phrases.iter().map(String::as_str))
            .collect();
        Checker {
            prechecks: Prechecks::new(extra),
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

    /// The options this checker judges with.
    pub fn options(&self) -> &Options {
        &self.options
    }

    /// Run only the free pre-checks. `recent` holds texts recently posted by
    /// the same author or thread; an identical comment is a duplicate.
    pub fn precheck(&self, comment: &Comment, recent: &[String]) -> Option<Verdict> {
        let start = Instant::now();
        let recent: Vec<String> = recent.iter().map(|r| normalize(r)).collect();
        let reason = self
            .prechecks
            .run(&comment.text, self.options.level, &recent)?;
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

    /// Judge one comment: free pre-checks first, then the models in order.
    /// When no model answers, the verdict is needs_review with reason
    /// model_unavailable, never a silent approve.
    pub async fn check(&self, comment: &Comment, recent: &[String]) -> Result<Verdict, Error> {
        let start = Instant::now();
        if let Some(v) = self.precheck(comment, recent) {
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
