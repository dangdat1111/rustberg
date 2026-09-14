-- amoro-rs metadata store schema
-- Mirrors the subset of Amoro AMS's own metastore tables (catalog_metadata,
-- table_metadata, optimizing_task_*) needed by the Rust core service.

CREATE TABLE IF NOT EXISTS catalogs (
    name        TEXT PRIMARY KEY,
    format      TEXT NOT NULL,
    warehouse   TEXT NOT NULL,
    properties  JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE TABLE IF NOT EXISTS namespaces (
    catalog     TEXT NOT NULL REFERENCES catalogs(name) ON DELETE CASCADE,
    levels      TEXT[] NOT NULL,
    PRIMARY KEY (catalog, levels)
);

CREATE TABLE IF NOT EXISTS tables (
    catalog               TEXT NOT NULL,
    namespace             TEXT[] NOT NULL,
    name                  TEXT NOT NULL,
    format                TEXT NOT NULL,
    location              TEXT NOT NULL,
    metadata_location     TEXT NOT NULL,
    current_snapshot_id   BIGINT,
    properties            JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (catalog, namespace, name),
    FOREIGN KEY (catalog, namespace) REFERENCES namespaces(catalog, levels) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS optimize_jobs (
    id              UUID PRIMARY KEY,
    catalog         TEXT NOT NULL,
    namespace       TEXT[] NOT NULL,
    table_name      TEXT NOT NULL,
    job_type        TEXT NOT NULL,
    status          TEXT NOT NULL,
    input_files     INT NOT NULL,
    output_files    INT,
    error           TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    FOREIGN KEY (catalog, namespace, table_name) REFERENCES tables(catalog, namespace, name) ON DELETE CASCADE
);

-- The optimizer worker polls this: oldest pending job, first.
CREATE INDEX IF NOT EXISTS idx_optimize_jobs_pending
    ON optimize_jobs (created_at)
    WHERE status = 'pending';
