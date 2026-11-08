# Build: official Rust image. Run: distroless cc (glibc, CA certs, no shell,
# no package manager) as the nonroot user. Both pinned by index digest, so one
# Dockerfile builds linux/amd64 and linux/arm64; Dependabot bumps tag + digest.

# rust:1.99.1-bookworm
FROM rust:1.99.1-bookworm@sha256:e6f2cff35d0f309acf8433b4f05878ed77092602c5065cc81801177da0bb3a9c AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY data/labeled.jsonl ./data/labeled.jsonl
# Registry and target dir live in BuildKit cache mounts, so a source-only
# change rebuilds the crate, not every dependency. The binary is copied out of
# the mount in the same step.
ENV CARGO_PROFILE_RELEASE_STRIP=symbols
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked \
    && cp target/release/replysieve /replysieve

# gcr.io/distroless/cc-debian12:nonroot
FROM gcr.io/distroless/cc-debian12:nonroot@sha256:9dac0a79194e45a7da0158a9c6da57b217585af0786db3845d1f0ec1a0dd182f
LABEL org.opencontainers.image.source="https://github.com/zeroonething/replysieve" \
      org.opencontainers.image.licenses="MIT OR Apache-2.0" \
      org.opencontainers.image.description="Judge comments in a fraction of a second: approve, reject or needs-review with a reason, from small decision models on OpenRouter. HTTP server, CLI and MCP server."
COPY LICENSE-MIT LICENSE-APACHE NOTICE /usr/share/doc/replysieve/
COPY --from=build /replysieve /usr/local/bin/replysieve
# nonroot (65532), numeric so Kubernetes runAsNonRoot can verify it.
USER 65532:65532
EXPOSE 8080
ENTRYPOINT ["replysieve"]
CMD ["serve", "--bind", "0.0.0.0:8080"]
