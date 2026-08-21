FROM rust:1.88-trixie AS builder
WORKDIR /build
COPY . .
RUN cargo build --locked --release --bins

FROM debian:trixie-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl bash netcat-openbsd libgomp1 \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system --gid 10001 astravector \
    && useradd --system --uid 10001 --gid astravector --home-dir /app --shell /usr/sbin/nologin astravector \
    && mkdir -p /app /models/bge-m3 \
    && chown -R astravector:astravector /app /models
WORKDIR /app
COPY --from=builder /build/target/release/astravector-runtime /usr/local/bin/astravector-runtime
COPY --from=builder /build/target/release/astravector-qdrant-publisher /usr/local/bin/astravector-qdrant-publisher
COPY --from=builder /build/target/release/astravector-lifecycle /usr/local/bin/astravector-lifecycle
COPY --from=builder /build/target/release/astravector-reconciliation /usr/local/bin/astravector-reconciliation
COPY config/application.yaml /app/config/application.yaml
COPY migrations /app/migrations
COPY docker/model-bootstrap.sh /usr/local/bin/astravector-model-bootstrap
COPY docker/entrypoint.sh /usr/local/bin/astravector-entrypoint
RUN chmod 0755 /usr/local/bin/astravector-model-bootstrap /usr/local/bin/astravector-entrypoint
ENV ASTRAVECTOR_CONFIG=/app/config/application.yaml \
    ASTRAVECTOR_MODEL_REPOSITORY_URL=https://nexus.astrabase.asia/repository/astra-models/astravector/bge-m3/baseline-v1 \
    ASTRAVECTOR_MODEL_DIR=/models/bge-m3 \
    ASTRAVECTOR_MODEL_PATH=/models/bge-m3/model.onnx \
    ASTRAVECTOR_TOKENIZER_PATH=/models/bge-m3/tokenizer.json \
    ASTRAVECTOR_MODEL_SHA256=f84251230831afb359ab26d9fd37d5936d4d9bb5d1d5410e66442f630f24435b \
    ASTRAVECTOR_MODEL_DATA_SHA256=1eebfb28493f67bba03ce0ef64bfdc7fc5a3bd9d7493f818bb1d78cd798416b4 \
    ASTRAVECTOR_TOKENIZER_SHA256=21106b6d7dab2952c1d496fb21d5dc9db75c28ed361a05f5020bbba27810dd08
EXPOSE 50051 9090
USER astravector:astravector
ENTRYPOINT ["astravector-entrypoint"]
CMD ["astravector-runtime"]
