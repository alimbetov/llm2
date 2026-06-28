# Known limitations

1. Код должен пройти Rust compilation and clippy; API `ort` и generated protobuf names требуют подтверждения.
2. Enrichment не генерирует summary/questions без подключённого provider.
3. Relevance baseline не заменяет cross-encoder и answer-source entailment.
4. Reconciliation worker по Qdrant scroll не завершён.
5. Qdrant collection provisioning пока эксплуатационная операция.
6. В архиве отсутствует сгенерированный `Cargo.lock`, так как Rust toolchain в среде сборки недоступен.
