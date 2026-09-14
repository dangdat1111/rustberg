# rustberg

A lightweight, memory-safe lakehouse management system built entirely in Rust. It provides a REST-compatible catalog service for Apache Iceberg tables, an async self-optimizing engine (compaction, sorting, deduplication) powered by Apache DataFusion, and a Kafka-backed change-log store for low-latency CDC-style reads. Designed as a low-footprint alternative to JVM-based lakehouse control planes, it aims to cut memory and GC overhead while staying compatible with the open Iceberg REST Catalog spec so existing query engines can adopt it with minimal changes.

## Layout

```
crates/
  core/        rustberg-core       domain model, error type, service traits (no I/O)
  metastore/   rustberg-metastore  PostgreSQL implementation of MetadataStore (sqlx)
  catalog/     rustberg-catalog    REST catalog service (axum) — namespaces, tables, commit
  optimizer/   rustberg-optimizer  self-optimizing engine: plan() + execute() per job
  logstore/    rustberg-logstore   change-log publishing: in-memory (default) or Kafka (feature = "kafka")
  api/         rustberg-api        binary: wires everything, runs the HTTP server + worker loop
migrations/    Postgres schema for catalogs/namespaces/tables/optimize_jobs
```

Each crate only depends on the trait boundaries in `rustberg-core`
(`MetadataStore`, `Optimizer`, `LogStore`), not on each other's concrete
types — swapping Postgres for something else, or the naive optimizer for a
DataFusion-backed one, touches one crate, not the whole tree.

## What's implemented vs. stubbed

**Implemented for real:**
- Full `MetadataStore` trait over PostgreSQL: catalogs, namespaces, tables,
  and an optimistic-concurrency `commit_table` (compare-and-swap on
  `current_snapshot_id`, the same protocol Iceberg catalogs use).
- A REST API covering catalog/namespace/table CRUD, table commit, and
  triggering an optimize job — routes are a practical subset of the
  [Iceberg REST Catalog spec](https://iceberg.apache.org/docs/latest/rest-catalog/wire-format/).
- A background worker (`crates/api/src/worker.rs`) that polls the
  `optimize_jobs` queue with `FOR UPDATE SKIP LOCKED`, so you can run
  multiple `rustberg-server` processes as optimizer workers without
  double-processing a job.
- An in-memory `LogStore` (broadcast channel) and a Kafka one behind the
  `kafka` feature flag on `rustberg-logstore`.

**Deliberately stubbed — this is the real next-step list, not TODOs
left by accident:**
- **Iceberg metadata I/O.** `create_table`/`commit_table` accept a
  metadata-file *location* but don't read/write Iceberg's
  `metadata.json` or manifest files yet. That's [`iceberg-rust`](https://github.com/apache/iceberg-rust)'s
  job — wire it into `rustberg-catalog`'s handlers once its write path is
  where you need it to be.
- **Compaction execution.** `rustberg-optimizer`'s `execute()` is a
  placeholder that marks jobs succeeded without touching data. A real
  executor (behind the `datafusion-exec` feature, dependency already
  declared) builds a DataFusion plan — `ParquetExec(inputs) → merge/sort
  → ParquetSink` — from the files `iceberg-rust` reports, then hands the
  new file list back to the catalog to commit.
- **Mixed-format (Amoro's own Iceberg/Hive extensions).** No Rust
  implementation exists anywhere yet; out of scope until the native
  Iceberg path is solid.
- **Flink/Spark connectors.** Not attempted here on purpose — see the
  architecture note below.

## Why there's no Flink/Spark connector crate

Flink and Spark are JVM-only; there's no way to "rewrite them in Rust."
The realistic shape is: Rust core service exposing gRPC, thin JVM shims
for Flink/Spark that call into it, and Trino talking to the REST catalog
natively (it already speaks the Iceberg REST spec). That JVM boundary is
a separate repo/module, not part of this Rust workspace.

## Running locally

```bash
docker compose up -d          # Postgres on :5432
cp .env.example .env
cargo run -p rustberg-api --bin rustberg-server
```

The server runs migrations automatically on startup
(`sqlx::migrate!` against `migrations/`).

```bash
# create a catalog
curl -X POST localhost:8080/v1/catalogs \
  -H 'content-type: application/json' \
  -d '{"name":"prod","format":"iceberg","warehouse":"s3://bucket/warehouse"}'

# create a namespace
curl -X POST localhost:8080/v1/prod/namespaces \
  -H 'content-type: application/json' -d '{"levels":["sales"]}'

# create a table
curl -X POST localhost:8080/v1/prod/namespaces/sales/tables \
  -H 'content-type: application/json' \
  -d '{"name":"orders","format":"iceberg","location":"s3://bucket/warehouse/sales/orders"}'

# trigger a minor compaction
curl -X POST localhost:8080/v1/prod/namespaces/sales/tables/orders/optimize \
  -H 'content-type: application/json' -d '{"job_type":"minor_compaction"}'
```

## A note on this sandbox's toolchain

This code was written and hand-reviewed in an environment where the only
available Rust toolchain is Ubuntu's packaged `rustc`/`cargo` 1.75
(`apt install rustc cargo`). That's old enough that `cargo check` can't
even resolve the dependency graph — several ordinary crates on crates.io
today (`getrandom`, `base64ct`, and others, pulled in transitively by
`uuid`/`sqlx`) now require Cargo's `edition2024` feature, which 1.75
doesn't understand. So `cargo check --workspace` could not be run to
completion here.

**This is a property of this sandbox, not of the code** — any machine
with a current stable toolchain (`rustup toolchain install stable`, or
whatever your CI already uses — anything ~1.80+) should resolve and
build cleanly. Please run `cargo check --workspace` as the first step
after pulling this down, and treat any errors it surfaces as real —
I couldn't verify compilation myself here, so review before relying on
it.

## Roadmap

1. Wire `iceberg-rust` into `rustberg-catalog` for real metadata.json /
   manifest read-write.
2. Implement the DataFusion executor in `rustberg-optimizer` behind
   `datafusion-exec`.
3. Add the gRPC surface (`tonic`) alongside REST for the future JVM
   connector shims.
4. Observability: OpenTelemetry traces around the worker loop and HTTP
   handlers.
