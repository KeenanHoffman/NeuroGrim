# syntax=docker/dockerfile:1.6
#
# Moth(er):Br+AI+n — External Brain reference deployment (S6-DB-5).
#
# Purpose: package `motherbrain a2a-serve` as a container so the Brain can run
# as an A2A peer anywhere Docker runs. Same binary the Phase E dual-brain
# integration test exercises (`motherbrain/crates/motherbrain-cli/tests/
# dual_brain_pair.rs`) — same wire protocol.
#
# Non-goals (explicit — spec §13.6 / deployment doc §7):
#   * No TLS termination. Add a reverse proxy for HTTPS.
#   * No auth beyond network-layer. v2.1 mandates authentication: none;
#     the adopter must gate access at the network layer.
#   * Not production-hardened for multi-tenant. Reference, not hardened kit.
#
# Build:   docker build -t motherbrain:dev .
# Run:     docker run -p 8421:8421 \
#            -v "$(pwd)/motherbrain-local-project:/brain:ro" \
#            motherbrain:dev
# Discover: motherbrain a2a-discover http://127.0.0.1:8421/a2a/v1/
#
# =============================================================================
# Stage 1 — Builder
# =============================================================================
# Pinned to rust:1.89 for reproducibility. `slim-bookworm` is Debian 12 slim
# (glibc-based) — keeps the target triple `x86_64-unknown-linux-gnu` and
# binaries compatible with the runtime stage below.
#
# Version floor: we need Rust >= 1.85 because `rmcp 0.8` requires the stable
# `edition2024` Cargo feature. 1.83 was the prompt's suggested default; it
# fails with "feature `edition2024` is required" on first build. 1.89 is
# a recent stable that satisfies that requirement; bump as needed when the
# workspace crates upstream their MSRV.
FROM rust:1.89-slim-bookworm AS builder

# Build-time packages only. `pkg-config` is harmless but required by some
# transitive crates; we do NOT install `libssl-dev` — our reqwest uses
# `rustls-tls` (see crates/motherbrain-a2a/Cargo.toml) so TLS is pure-Rust.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        pkg-config \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /work

# -----------------------------------------------------------------------------
# Dependency-cache layer.
#
# The standard Rust multi-stage trick: copy only Cargo manifests + lockfile
# plus tiny stub `src/main.rs` / `src/lib.rs` files, then `cargo build
# --release` so every crate in Cargo.lock is compiled and cached in this
# layer. When application source changes later, this layer stays cached
# and only the final link step reruns.
#
# We mirror the workspace's crate layout so `cargo build --workspace`
# resolves correctly against the manifests.
# -----------------------------------------------------------------------------
COPY motherbrain/Cargo.toml motherbrain/Cargo.lock ./motherbrain/

COPY motherbrain/crates/motherbrain-core/Cargo.toml      ./motherbrain/crates/motherbrain-core/
COPY motherbrain/crates/motherbrain-sensory/Cargo.toml   ./motherbrain/crates/motherbrain-sensory/
COPY motherbrain/crates/motherbrain-mcp/Cargo.toml       ./motherbrain/crates/motherbrain-mcp/
COPY motherbrain/crates/motherbrain-a2a/Cargo.toml       ./motherbrain/crates/motherbrain-a2a/
COPY motherbrain/crates/motherbrain-ecosystem/Cargo.toml ./motherbrain/crates/motherbrain-ecosystem/
COPY motherbrain/crates/motherbrain-cli/Cargo.toml       ./motherbrain/crates/motherbrain-cli/

# Stub source trees so `cargo build` resolves — they'll be overwritten below.
# Library crates need `src/lib.rs`; the CLI binary needs `src/main.rs`.
RUN set -eux; \
    for c in motherbrain-core motherbrain-sensory motherbrain-mcp motherbrain-a2a motherbrain-ecosystem; do \
        mkdir -p "./motherbrain/crates/${c}/src"; \
        echo '// dep-cache stub' > "./motherbrain/crates/${c}/src/lib.rs"; \
    done; \
    mkdir -p ./motherbrain/crates/motherbrain-cli/src; \
    echo 'fn main() {}' > ./motherbrain/crates/motherbrain-cli/src/main.rs

# Warm the dependency layer. Release build so we cache the same artifacts
# the final build reuses. A stub-source build of the CLI binary alone
# forces cargo to compile all transitive deps.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/work/motherbrain/target \
    cd motherbrain \
    && cargo build --release -p motherbrain-cli

# -----------------------------------------------------------------------------
# Real application source.
#
# Overwrite the stubs with the real crates and rebuild. Cache-invalidation
# hazard: the stub build above produced `motherbrain-cli v0.1.0` and the
# workspace library crates against their stub sources; cargo fingerprints
# them by source content, and if we just `touch` Cargo.toml it can still
# skip the rebuild because the library stubs looked like "valid empty
# crates" to cargo — yielding a tiny "stub" binary in the final image.
# Seen in practice: image shipped a 436 KB `motherbrain` that printed
# nothing because it was the stub.
#
# Belt-and-braces fix: (1) delete the target artifacts for every workspace
# crate so cargo must recompile them against the real source; (2) rebuild.
# We leave the registry cache intact so external deps aren't re-fetched.
# -----------------------------------------------------------------------------
COPY motherbrain/crates ./motherbrain/crates

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/work/motherbrain/target \
    cd motherbrain \
    && for p in motherbrain-core motherbrain-sensory motherbrain-mcp \
               motherbrain-a2a motherbrain-ecosystem motherbrain-cli; do \
          cargo clean --release -p "$p" || true; \
       done \
    && cargo build --release -p motherbrain-cli \
    && cp target/release/motherbrain /work/motherbrain-bin \
    && ls -la /work/motherbrain-bin

# Sanity: confirm the binary runs and *is* the real CLI (not a stub). The
# bugfix history here is that a stub-source cache hit once shipped a
# 436 KB no-op binary; hence the two checks:
#   (1) `--version` prints a non-empty version string, and
#   (2) the `a2a-serve` subcommand is present in `--help` output.
# A stub binary produced by `fn main() {}` passes neither.
RUN set -eux \
    && VERSION_OUT="$(/work/motherbrain-bin --version)" \
    && [ -n "$VERSION_OUT" ] \
    && echo "built binary version: $VERSION_OUT" \
    && /work/motherbrain-bin --help | grep -q 'a2a-serve'

# =============================================================================
# Stage 2 — Runtime
# =============================================================================
# `debian:bookworm-slim` — small, matches builder's glibc, has the CA bundle
# so outbound HTTPS works (rustls reads the system store via webpki-roots;
# we still ship ca-certificates to be explicit about trust material).
FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Non-root user per hardening guidelines. UID 1000 is a common convention
# and avoids colliding with system users. `--system` would use a lower
# UID; we want this recognisable from the host when volume-mounting.
RUN groupadd --system --gid 1000 brain \
    && useradd --system --uid 1000 --gid brain --home-dir /home/brain --create-home brain

# Default mount point for the project root. `:ro` in the run/compose
# invocations keeps the container from scribbling on the host's project.
RUN mkdir -p /brain && chown brain:brain /brain

COPY --from=builder /work/motherbrain-bin /usr/local/bin/motherbrain

# Health-check tool isn't installed — keeping the image small. Operators
# can `docker exec` if they need to probe; compose healthchecks can use
# the server's /.well-known endpoint with a sidecar curl if desired.

EXPOSE 8421
USER brain
WORKDIR /home/brain

# `a2a-serve` with the container defaults:
#   --bind 0.0.0.0   — port reachable from outside the container namespace.
#                       Spec §13.6: v2.1 mandates authentication: none;
#                       operator MUST gate access at the network layer.
#   --port 8421      — matches `DEFAULT_PORT` in a2a_serve.rs.
#   --project-root   — mounted at /brain (see docker-compose.yml).
#
# Using ENTRYPOINT + CMD split so operators can override args via
# `docker run motherbrain:dev <other-subcommand>` without re-typing the
# binary path.
ENTRYPOINT ["/usr/local/bin/motherbrain"]
CMD ["a2a-serve", "--bind", "0.0.0.0", "--port", "8421", "--project-root", "/brain"]
