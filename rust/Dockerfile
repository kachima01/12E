FROM rust:1-bookworm AS build

RUN rustup target add wasm32-unknown-unknown

RUN curl --fail --location --silent --show-error \
      https://github.com/trunk-rs/trunk/releases/download/v0.22.1/trunk-x86_64-unknown-linux-musl.tar.gz \
      --output /tmp/trunk.tar.gz \
    && echo "a1fe5cd6b053f5a76f66b598d296022a7f06f56def6013cc16cd6897d53f5363  /tmp/trunk.tar.gz" | sha256sum --check \
    && tar -xzf /tmp/trunk.tar.gz -C /usr/local/cargo/bin trunk \
    && rm /tmp/trunk.tar.gz

RUN cargo install worker-build --version 0.8.7 --locked --jobs 4

WORKDIR /workspace
COPY rust ./rust

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/workspace/rust/target \
    cd rust/frontend && trunk build --release
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/workspace/rust/target \
    cd rust/worker && worker-build --release

FROM node:22-bookworm-slim AS runtime

RUN npm install --global wrangler@4
WORKDIR /app

COPY rust/worker/wrangler.toml ./worker/wrangler.toml
COPY --from=build /workspace/rust/worker/build ./worker/build
COPY --from=build /workspace/rust/frontend/dist ./frontend/dist

EXPOSE 8787
CMD ["wrangler", "dev", "--ip", "0.0.0.0", "--port", "8787", "--config", "/app/worker/wrangler.toml"]
