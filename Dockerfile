FROM docker.io/library/rust:1.79.0-bullseye AS build

ARG DATABASE_URL

WORKDIR /app

COPY Cargo.toml Cargo.lock .
COPY models/Cargo.toml models/Cargo.toml
COPY fluorescence_scan/Cargo.toml fluorescence_scan/Cargo.toml

RUN mkdir models/src \
    && touch models/src/lib.rs \
    && mkdir fluorescence_scan/src \
    && echo "fn main() {}" > fluorescence_scan/src/main.rs \
    && cargo build --release

COPY . /app

RUN touch models/src/lib.rs \
    && touch fluorescence_scan/src/main.rs \
    && cargo build --release

FROM gcr.io/distroless/cc AS deploy

COPY --from=build /app/target/release/fluorescence_scan /fluorescence_scan

ENTRYPOINT ["/fluorescence_scan"]
