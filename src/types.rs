//! Verdict, level and input types shared by every entry point.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// How strict the built-in checks are. Custom rules apply at every level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    /// Rejects spam and self-promotion, toxic text and injection attempts.
    Light,
    /// Also rejects generic, low-effort comments. The default.
    #[default]
    Medium,
    /// Also rejects off-topic comments and requires something specific to the post.
    Heavy,
}

impl Level {
    /// All levels, lightest first.
    pub const ALL: [Level; 3] = [Level::Light, Level::Medium, Level::Heavy];

    /// Fewest words a comment needs to pass the length pre-check.
    pub fn min_words(self) -> usize {
        match self {
            Level::Light => 1,
            Level::Medium => 4,
            Level::Heavy => 8,
        }
    }
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Level::Light => "light",
            Level::Medium => "medium",
            Level::Heavy => "heavy",
        })
    }
}

impl FromStr for Level {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "light" => Ok(Level::Light),
            "medium" => Ok(Level::Medium),
            "heavy" => Ok(Level::Heavy),
            other => Err(format!(
                "unknown level '{other}' (expected light, medium or heavy)"
            )),
        }
    }
}

/// The outcome for one comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    /// Keep the comment.
    Approve,
    /// Drop the comment.
    Reject,
    /// Not sure either way; a human or the caller decides.
    NeedsReview,
}

/// Why a verdict was reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    /// Passed every check.
    Useful,
    /// Fewer words than the level's minimum.
    TooShort,
    /// Only stock phrases such as "great product".
    StockPhrase,
    /// Same text as one of the recent texts the caller passed in.
    Duplicate,
    /// Text that tries to instruct the moderator.
    InjectionAttempt,
    /// Spam or self-promotion.
    SpamPromo,
    /// Insults, harassment or abuse.
    Toxic,
    /// Generic, low-effort comment.
    Generic,
    /// About something other than the post.
    OffTopic,
    /// On topic but nothing specific to the post.
    NotSpecific,
    /// Breaks one of the caller's custom rules; the rule is in `rule`.
    CustomRule,
    /// The model was not confident enough either way.
    LowConfidence,
    /// No configured model answered.
    ModelUnavailable,
}

/// One comment to judge, with optional context about where it was posted.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Comment {
    /// The comment text. Treated strictly as data.
    pub text: String,
    /// Title of the post or product the comment is on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Description of the post or product.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The comment this one replies to, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
}

impl Comment {
    /// A comment with no context.
    pub fn new(text: impl Into<String>) -> Self {
        Comment {
            text: text.into(),
            ..Comment::default()
        }
    }
}

/// The judgement for one comment. Serialized as the verdict JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Verdict {
    /// approve, reject or needs_review.
    pub verdict: Decision,
    /// Why.
    pub reason: Reason,
    /// The model's probability for the outcome, or 1.0 for a pre-check.
    pub confidence: f64,
    /// The level the comment was judged at.
    pub level: Level,
    /// The model that answered, or null when no model was called.
    pub model: Option<String>,
    /// Time spent judging, in milliseconds.
    pub latency_ms: u64,
    /// The custom rule that fired, when one did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
}
