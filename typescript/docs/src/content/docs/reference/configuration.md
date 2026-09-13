---
title: Configuration
description: Every flag and environment variable of guru-master, guru-worker, manage-tool and the dashboard.
---

Each binary takes the same value from a CLI flag or an environment variable; the flag wins.

## `guru-master`

| Flag | Environment | Default |
|---|---|---|
| `--mode` | `GURU_WORKER_MODE` | `dashboard_grpc` |
| `--dashboard-addr` | `GURU_DASHBOARD_GRPC_ADDR` | `0.0.0.0:50051` |
| `--workers-addr` | `GURU_WORKERS_GRPC_ADDR` | `0.0.0.0:50052` |
| `--address` | `SURREALDB_HOST` | `ws://127.0.0.1:8000` |
| `--username` | `SURREALDB_USER` | `root` |
| `--password` | `SURREALDB_PASSWORD` | `root` |
| `--namespace` | `SURREALDB_NAMESPACE` | *required* |
| `--database` | `SURREALDB_NAME` | *required* |
| `--amqp-uri` | `AMQP_URI` | *required in `dashboard_grpc`, `workers_grpc`, `consumer`* |
| `--sweep-interval-secs` | `GURU_SWEEP_INTERVAL_SECS` | `30` (must be ≥ 1) |
| `--watch-poll-ms` | `GURU_WATCH_POLL_MS` | `1000` (must be ≥ 1) |
| `--log-level` | `GURU_LOG_LEVEL` | `info` |

`--mode` accepts `dashboard_grpc`, `workers_grpc`, `consumer` and `cron`. A broker URI looks like
`amqp://guru:guru@127.0.0.1:5672/`, where the trailing `/` selects the default vhost.

## `guru-worker`

| Flag | Environment | Default |
|---|---|---|
| `-c`, `--config` | `GURU_WORKER_CONFIG` | — (standalone mode; reloaded on `SIGHUP`) |
| `--master` | `GURU_MASTER` | — (agent mode; requires `--server`) |
| `--server` | `GURU_SERVER_ID` | — (`orchestration_server` record key) |
| `--api-key-file` | `GURU_API_KEY_FILE` | — (alternative to `GURU_API_KEY`) |
| `--state-dir` | `GURU_STATE_DIR` | `/var/lib/guru-worker` |
| `--log-level` | `GURU_LOG_LEVEL` | `info` |

`--config` and `--master` are mutually exclusive. Agent mode reads the operator API key from
`GURU_API_KEY`, or from the file given by `--api-key-file` (trailing whitespace is trimmed); the key
is used once per session to register with the master.

## `manage-tool`

Global flags mirror `guru-master`'s database options: `--address` (`SURREALDB_HOST`), `--username`
(`SURREALDB_USER`), `--password` (`SURREALDB_PASSWORD`), `--namespace` (`SURREALDB_NAMESPACE`),
`--database` (`SURREALDB_NAME`).

| Subcommand | Purpose |
|---|---|
| `create-admin --email <email> --password <password>` | Bootstrap the first administrator account |
| `orchestration export-config --server <key>` | Print the derived `guru-worker` TOML for one server |

## Dashboard

| Environment | Default | Purpose |
|---|---|---|
| `GURU_GRPC_URL` | `127.0.0.1:50051` | Dashboard gRPC endpoint of `guru-master --mode dashboard_grpc` |
| `ORIGIN` | — | Public URL when running behind a reverse proxy (or use `PROTOCOL_HEADER` / `HOST_HEADER`) |

The server listens on `:3000`.

## Module configuration

Typed operator settings are *not* environment variables. Each module declares a
`serde`-(de)serializable struct implementing `Default`, bound to a stable string key; the value is
stored as JSON in the database, cached in Redis, and seeded by `manage-tool`.
