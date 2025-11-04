FROM rust:latest AS builder

WORKDIR /src
COPY Cargo.toml /src/
COPY migrations/ /src/migrations/
COPY src/ /src/src/
RUN cargo install --path . --root /usr/local

FROM builder AS cadence-migrate-up
ENTRYPOINT ["/usr/local/bin/cadence-migrate-up"]

FROM builder AS cadence-serve
ENTRYPOINT ["/usr/local/bin/cadenced"]
