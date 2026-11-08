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
cargo-deny and a secret scan on every pull request.

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
