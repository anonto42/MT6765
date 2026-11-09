# Local CI image: pinned Rust + aarch64 musl target + Xvfb (X bridge tests) + qemu-user (smoke).
ARG RUST_VERSION
FROM rust:${RUST_VERSION}-slim
RUN apt-get update && apt-get install -y --no-install-recommends xvfb xauth qemu-user file \
    && rm -rf /var/lib/apt/lists/* \
    && rustup component add rustfmt clippy \
    && rustup target add aarch64-unknown-linux-musl
