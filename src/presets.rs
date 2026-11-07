//! Built-in presets: what kind of place the comments come from.

use std::str::FromStr;

/// A built-in description of where comments are posted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Preset {
    /// Comments under a post or article.
    #[default]
    Comments,
    /// Reviews of a product.
    ProductReview,
    /// Comments in a product launch thread.
    LaunchThread,
}

impl Preset {
    /// Every preset name, as accepted on the command line.
    pub const NAMES: [&'static str; 3] = ["comments", "product-review", "launch-thread"];

    /// The preset's name.
    pub fn name(self) -> &'static str {
        match self {
            Preset::Comments => "comments",
            Preset::ProductReview => "product-review",
            Preset::LaunchThread => "launch-thread",
        }
    }

    /// What the comment is, in words the model reads.
    pub fn setting(self) -> &'static str {
        match self {
            Preset::Comments => "a comment under a post",
            Preset::ProductReview => "a user review of a product",
            Preset::LaunchThread => "a comment in a product launch thread",
        }
    }

    /// Stock phrases typical of this setting, on top of the shared list.
    pub fn stock_phrases(self) -> &'static [&'static str] {
        match self {
            Preset::Comments => &[
                "great post",
                "nice post",
                "thanks for sharing",
                "great read",
            ],
            Preset::ProductReview => &[
                "great product",
                "love it",
                "highly recommend",
                "five stars",
                "works great",
                "best app ever",
            ],
            Preset::LaunchThread => &[
                "congrats on the launch",
                "congrats on launching",
                "happy launch day",
                "good luck with the launch",
                "upvoted",
                "great launch",
            ],
        }
    }
}

impl FromStr for Preset {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "comments" => Ok(Preset::Comments),
            "product-review" => Ok(Preset::ProductReview),
            "launch-thread" => Ok(Preset::LaunchThread),
            other => Err(format!(
                "unknown preset '{other}' (expected one of: {})",
                Preset::NAMES.join(", ")
            )),
        }
    }
}
