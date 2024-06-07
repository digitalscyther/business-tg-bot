# syntax=docker/dockerfile:1

ARG RUST_VERSION=1.78.0
ARG APP_NAME=bot

FROM rust:${RUST_VERSION}-slim AS build
ARG APP_NAME
WORKDIR /app

RUN apt-get update && \
    apt-get install -y clang lld musl-tools pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/*

RUN --mount=type=bind,source=src,target=src \
    --mount=type=bind,source=migrations,target=migrations \
    --mount=type=bind,source=.sqlx,target=.sqlx \
    --mount=type=bind,source=Cargo.toml,target=Cargo.toml \
    --mount=type=bind,source=Cargo.lock,target=Cargo.lock \
    --mount=type=cache,target=/app/target/ \
    --mount=type=cache,target=/usr/local/cargo/git/db \
    --mount=type=cache,target=/usr/local/cargo/registry/ \
cargo build --locked --release && \
cp ./target/release/$APP_NAME /bin/bot

FROM debian:bookworm-slim AS final

ARG UID=10001
RUN apt-get update && \
    apt-get install -y libssl3 ca-certificates && \
    rm -rf /var/lib/apt/lists/*

RUN adduser \
    --disabled-password \
    --gecos "" \
    --home "/nonexistent" \
    --shell "/sbin/nologin" \
    --no-create-home \
    --uid "${UID}" \
    appuser
USER appuser

COPY --from=build /bin/bot /bin/
COPY files files

CMD ["/bin/bot"]