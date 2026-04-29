FROM rust:1.82-alpine AS chef
RUN apk add --no-cache musl-dev
RUN cargo install cargo-chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release --bin portal-server

FROM alpine:3.20
RUN apk add --no-cache ca-certificates
COPY --from=builder /app/target/release/portal-server /usr/local/bin/portal-server
ENV PORT=8080
ENV HOST=0.0.0.0
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/portal-server"]
