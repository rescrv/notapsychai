FROM rust:latest AS builder

WORKDIR /src
COPY . /src
RUN cargo install --path cadence --root /usr/local

FROM ubuntu:24.04 AS cadence-base
COPY --from=builder /usr/local /usr/local

FROM cadence-base AS cadence-migrate-up
ENTRYPOINT ["/usr/local/bin/cadence-migrate-up"]

FROM cadence-base AS cadence-serve
ENTRYPOINT ["/usr/local/bin/cadenced"]
