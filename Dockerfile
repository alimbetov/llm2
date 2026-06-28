FROM rust:1.82-bookworm AS builder
WORKDIR /build
COPY . .
RUN cargo build --release --bins --bins

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libgomp1 && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /build/target/release/astravector-runtime /usr/local/bin/astravector-runtime
COPY --from=builder /build/target/release/astravector-qdrant-publisher /usr/local/bin/astravector-qdrant-publisher
COPY --from=builder /build/target/release/astravector-lifecycle /usr/local/bin/astravector-lifecycle
COPY config/application.yaml /app/config/application.yaml
COPY migrations /app/migrations
ENV ASTRAVECTOR_CONFIG=/app/config/application.yaml
EXPOSE 50051 9090
ENTRYPOINT ["astravector-runtime"]
