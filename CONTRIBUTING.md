# Contributing to replysieve

Thanks for helping. Bug reports, wrong verdicts, new labeled examples and code
are all welcome.

## Reporting a wrong verdict

Open an issue with the comment text (or a close paraphrase if it is private), the
context you passed, the level and preset, and the verdict JSON you got. Wrong
verdicts are the most useful reports this project gets.

## Development

You need stable Rust (1.90 or newer).

```sh
cargo test                                   # unit and integration tests, no network
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cargo deny check                             # licenses and advisories
```

The tests run against a mocked OpenRouter server, so they need no API key. To
measure accuracy against the real model, set `OPENROUTER_API_KEY` and run
`cargo run --release -- eval`.

Keep changes small and focused, add a test for any behaviour you change, and
never log or write comment text anywhere. `tests/no_content_logged.rs` checks
this.

## Pull requests

Open pull requests against the `develop` branch. CI runs tests, clippy, fmt,
cargo-deny, a secret scan, and a build and smoke test of the Docker image on
every pull request.

## Releases

Releases run themselves from `main`. To cut one:

1. On `develop`, set the new `version` in `Cargo.toml` (run `cargo build` so
   `Cargo.lock` follows) and add its section at the top of `CHANGELOG.md`.
2. Merge `develop` into `main`.

The push to `main` starts `.github/workflows/release.yml`. When that version has
no release yet, it runs the full test suite, creates the `vX.Y.Z` tag and the
GitHub release from the CHANGELOG section, publishes to crates.io, and pushes
the `linux/amd64` and `linux/arm64` image to `ghcr.io/zeroonething/replysieve`
as `X.Y.Z`, `X.Y` and `latest`. A push to `main` without a version bump
releases nothing.

Every step first checks whether its output already exists, so a release that
stops half way is finished by re-running the failed jobs, or with "Run workflow"
on `main`. Once the tag exists, everything is built from the tagged commit.

## Developer Certificate of Origin

Every commit must be signed off, which certifies that you wrote the change or
otherwise have the right to submit it under the project's licenses, as described
in the [Developer Certificate of Origin](https://developercertificate.org/).

```sh
git commit -s -m "Fix stock-phrase matching for trailing emoji"
```

That adds a line like `Signed-off-by: Your Name <you@example.com>` to the
commit message. Use your real name and an email you can be reached at.

## License

By contributing you agree that your contributions are dual licensed under the
MIT license and the Apache License 2.0, as the project is, without any
additional terms or conditions.
