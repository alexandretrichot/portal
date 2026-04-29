FROM rust:1.94-alpine AS builder

RUN apk add --no-cache musl-dev

WORKDIR /app
COPY . .

RUN cargo build --release --bin portal-server

FROM alpine:3.20

RUN apk add --no-cache ca-certificates

COPY --from=builder /app/target/release/portal-server /usr/local/bin/portal-server

ENV PORT=8080
ENV HOST=0.0.0.0

EXPOSE 8080

ENTRYPOINT ["/usr/local/bin/portal-server"]
