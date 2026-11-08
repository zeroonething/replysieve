//! The free pre-checks answer in under 5 ms at p99.
//! Run `cargo test --release --test precheck_latency -- --nocapture` for the benchmark figure.

use replysieve::{Checker, Comment, Level, Options, Preset};
use std::time::{Duration, Instant};

#[test]
fn precheck_p99_under_5ms() {
    let texts: Vec<String> = include_str!("../data/labeled.jsonl")
        .lines()
        .map(|l| {
            serde_json::from_str::<serde_json::Value>(l).unwrap()["text"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    let recent: Vec<String> = texts.iter().take(50).cloned().collect();
    let mut samples = Vec::new();
    for level in Level::ALL {
        for preset in [
            Preset::Comments,
            Preset::ProductReview,
            Preset::LaunchThread,
        ] {
            let checker = Checker::new(
                Options {
                    level,
                    preset,
                    ..Options::default()
                },
                None,
            );
            for _ in 0..5 {
                for t in &texts {
                    let c = Comment::new(t.clone());
                    let start = Instant::now();
                    std::hint::black_box(checker.precheck(&c, &recent));
                    samples.push(start.elapsed());
                }
            }
        }
    }
    samples.sort();
    let p = |q: f64| samples[((samples.len() as f64 * q) as usize).min(samples.len() - 1)];
    let (p50, p99, max) = (p(0.50), p(0.99), *samples.last().unwrap());
    println!(
        "precheck over {} calls: p50 {p50:?}, p99 {p99:?}, max {max:?}",
        samples.len()
    );
    assert!(p99 < Duration::from_millis(5), "p99 {p99:?}");
}
