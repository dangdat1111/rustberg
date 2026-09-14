# rustberg
A lightweight, memory-safe lakehouse management system built entirely in Rust. It provides a REST-compatible catalog service for Apache Iceberg tables, an async self-optimizing engine (compaction, sorting, deduplication) powered by Apache DataFusion, and a Kafka-backed change-log store for low-latency CDC-style reads. Designed as a low-footprint alternative to JVM-based lakehouse control planes, it aims to cut memory and GC overhead while staying compatible with the open Iceberg REST Catalog spec so existing query engines can adopt it with minimal changes.

