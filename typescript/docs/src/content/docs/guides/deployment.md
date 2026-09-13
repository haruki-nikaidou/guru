---
title: Deployment
description: Container images, release tags and the environment each binary expects.
---

## Images

Both images build from the **repository root** — the frontend needs the whole Bun workspace for the
`app-protobuf` package:

```sh
docker build -f master.Dockerfile   -t guru-master   .
docker build -f frontend.Dockerfile -t guru-frontend .
```

## Release tags

Only tag pushes publish. `<version>` is the tag minus its prefix; `latest` moves only for a final
`vX.Y.Z`.

| Tag | Publishes |
|---|---|
| `master-v0.1.0[-alpha]` | `ghcr.io/haruki-nikaidou/guru-master:<version>` (`distroless/cc-debian13:nonroot`) |
| `frontend-v0.1.0[-alpha]` | `ghcr.io/haruki-nikaidou/guru-frontend:<version>` (`distroless/nodejs24-debian13:nonroot`) |
| `worker-v0.1.0[-alpha]` | GitHub release with the raw `linux/x86_64` `guru-worker` binary |

## Running the control plane

`guru-master` is configured entirely through the environment. `GURU_WORKER_MODE` selects the mode;
`SURREALDB_NAMESPACE`, `SURREALDB_NAME` and `AMQP_URI` have **no defaults**. Run one container per
mode:

- `dashboard_grpc` — operator API, listens on `GURU_DASHBOARD_GRPC_ADDR` (`0.0.0.0:50051`).
- `workers_grpc` — worker API, listens on `GURU_WORKERS_GRPC_ADDR` (`0.0.0.0:50052`).
- `consumer` — AMQP derivation hook.
- `cron` — stale-canvas sweep.

See [Configuration](/reference/configuration/) for the full variable list.

## Running the dashboard

The frontend listens on `:3000` and reaches the control plane through `GURU_GRPC_URL`. Behind a
reverse proxy, set `ORIGIN` (or `PROTOCOL_HEADER` / `HOST_HEADER`) so the Node adapter builds
correct URLs and passes its CSRF check.

## Running a worker

Standalone, from a file that is reloaded on `SIGHUP`:

```sh
guru-worker --config /etc/guru/worker.toml
```

Agent mode, streaming configs from the master:

```sh
GURU_API_KEY=<operator-api-key> \
guru-worker --master http://10.0.0.1:50052 --server <orchestration_server key>
```

The API key is used once per session to register with the master; `--api-key-file`
(`GURU_API_KEY_FILE`) is the file-based alternative. Worker state is kept in `GURU_STATE_DIR`
(`/var/lib/guru-worker`).

## Schema rollouts

Development uses `surrealkit sync`. For shared or production databases use `surrealkit rollout`,
which plans the change set and supports rollback.
