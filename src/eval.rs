//! `replysieve eval`: accuracy per level over a labeled set.

use anyhow::{Context, bail};
use futures_util::{StreamExt, stream};
use replysieve::{Checker, Comment, Decision, Level, Preset};
use serde::Deserialize;

/// The labeled set shipped with replysieve.
pub const BUILT_IN: &str = include_str!("../data/labeled.jsonl");

const CONCURRENCY: usize = 8;

#[derive(Deserialize)]
struct Example {
    id: String,
    source: String,
    category: String,
    text: String,
    #[serde(default)]
    context: Context_,
}

#[derive(Deserialize, Default)]
struct Context_ {
    title: Option<String>,
    description: Option<String>,
    parent: Option<String>,
}

const CATEGORIES: [&str; 7] = [
    "specific",
    "nonspecific",
    "off_topic",
    "generic",
    "spam_promo",
    "toxic",
    "injection",
];

/// Whether a comment of `category` should be kept at `level`.
fn is_good(category: &str, level: Level) -> bool {
    match level {
        Level::Light => matches!(
            category,
            "specific" | "nonspecific" | "off_topic" | "generic"
        ),
        Level::Medium => matches!(category, "specific" | "nonspecific" | "off_topic"),
        Level::Heavy => category == "specific",
    }
}

fn preset_for(source: &str) -> Preset {
    match source {
        "review" => Preset::ProductReview,
        "launch" => Preset::LaunchThread,
        _ => Preset::Comments,
    }
}

fn parse(text: &str) -> anyhow::Result<Vec<Example>> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let e: Example =
            serde_json::from_str(line).with_context(|| format!("labeled set line {}", i + 1))?;
        if !CATEGORIES.contains(&e.category.as_str()) {
            bail!(
                "labeled set line {}: unknown category '{}'",
                i + 1,
                e.category
            );
        }
        out.push(e);
    }
    Ok(out)
}

/// Counts for one level.
#[derive(Debug, Default, PartialEq)]
pub struct Score {
    pub junk: usize,
    pub junk_caught: usize,
    pub good: usize,
    pub good_rejected: usize,
    pub needs_review: usize,
}

fn pct(n: usize, d: usize) -> f64 {
    if d == 0 {
        0.0
    } else {
        100.0 * n as f64 / d as f64
    }
}

/// Run the set at each level and print one line per level.
/// `preset` overrides the per-source preset; `misses` prints each wrong verdict.
pub async fn run(
    checker: &Checker,
    set: &str,
    levels: &[Level],
    preset: Option<Preset>,
    misses: bool,
) -> anyhow::Result<Vec<(Level, Score)>> {
    let examples = parse(set)?;
    let mut results = Vec::new();
    println!(
        "{:<7} {:>22} {:>26} {:>13}",
        "level", "junk caught", "good wrongly rejected", "needs review"
    );
    for &level in levels {
        let checkers: Vec<Checker> = [
            Preset::Comments,
            Preset::ProductReview,
            Preset::LaunchThread,
        ]
        .into_iter()
        .map(|p| {
            let mut o = checker.options().clone();
            o.level = level;
            o.preset = preset.unwrap_or(p);
            checker.with_options(o)
        })
        .collect();
        let pick = |source: &str| match preset.unwrap_or(preset_for(source)) {
            Preset::Comments => &checkers[0],
            Preset::ProductReview => &checkers[1],
            Preset::LaunchThread => &checkers[2],
        };
        let verdicts: Vec<_> = stream::iter(&examples)
            .map(|e| {
                let c = Comment {
                    text: e.text.clone(),
                    title: e.context.title.clone(),
                    description: e.context.description.clone(),
                    parent: e.context.parent.clone(),
                };
                let checker = pick(&e.source);
                async move { checker.check(&c, &[]).await }
            })
            .buffered(CONCURRENCY)
            .collect()
            .await;
        let mut s = Score::default();
        for (e, v) in examples.iter().zip(verdicts) {
            let v = v?;
            let good = is_good(&e.category, level);
            let rejected = v.verdict == Decision::Reject;
            if v.verdict == Decision::NeedsReview {
                s.needs_review += 1;
            }
            if good {
                s.good += 1;
                s.good_rejected += rejected as usize;
            } else {
                s.junk += 1;
                s.junk_caught += rejected as usize;
            }
            if misses && (good == rejected || v.verdict == Decision::NeedsReview) {
                eprintln!(
                    "miss {level} {} {}: {:?} {:?} {:.2}",
                    e.id, e.category, v.verdict, v.reason, v.confidence
                );
            }
        }
        println!(
            "{:<7} {:>13.1}% ({:>3}/{:>3}) {:>17.1}% ({:>3}/{:>3}) {:>13}",
            level.to_string(),
            pct(s.junk_caught, s.junk),
            s.junk_caught,
            s.junk,
            pct(s.good_rejected, s.good),
            s.good_rejected,
            s.good,
            s.needs_review
        );
        results.push((level, s));
    }
    Ok(results)
}
