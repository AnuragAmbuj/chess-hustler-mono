# Multi-stage build for hustle-server (ADR-012).
# Build: docker build -f infra/docker/server.Dockerfile .

FROM rust:1-slim AS build
WORKDIR /src
COPY server/ .
RUN cargo build --release -p hustle-server

FROM gcr.io/distroless/cc-debian12
COPY --from=build /src/target/release/hustle-server /usr/local/bin/hustle-server
ENV CH_PORT=8080 \
    CH_ENV=prod
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/hustle-server"]
