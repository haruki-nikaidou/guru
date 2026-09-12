# Proxy Guru

A managed TCP/TLS proxy fabric: operators design a topology on a **canvas**, the
control plane derives one config per server from it, and data-plane workers pick
up every new revision automatically.

## Pieces

| Crate | Role |
|---|---|
| `bin/guru-master` | Control plane. One binary, four modes (`--mode`): `dashboard_grpc` (operator API), `workers_grpc` (worker API + config-view poller), `consumer` (AMQP derivation hook), `cron` (stale-canvas sweep). |
| `bin/guru-worker` | Data plane. Terminates listeners and forwards traffic. Runs standalone from a TOML file (reloaded on SIGHUP) or in agent mode, streaming configs from the master. |
| `bin/manage-tool` | Admin CLI: `create-admin` bootstrap, `orchestration export-config`. |
| `lib/guru_worker_config` | The worker config model, shared by both planes: the master derives it, the worker consumes it. |
| `lib/rpguru_sdk` | Generated gRPC/protobuf types (Rust) from `proto/`. |
| `modules/auth` | Accounts, sessions, API keys, RBAC. |
| `modules/orchestration` | Canvases, servers, nodes, edges; topology validation, config derivation and worker rollout. |
| `modules/notify` | Notification module — scaffolded from `base`, not implemented yet. |
| `modules/base` | Shared foundations and the layout every module mirrors. |

## Data plane

Each forwarding has a listener (`raw`, `tls`, or an inbound relay) and a
destination: a direct **exit**, a **relay** to another node over TLS-over-TCP or
QUIC, or a **load-balance** group. PROXY protocol v1/v2 is supported on both
ends.

## Rollout model

Mutations bump the canvas generation and publish `CanvasDirty`; the derivation
hook re-derives the whole canvas (a cron sweep catches anything a lost message
missed). Every server has one config view holding three snapshots — `desired`,
`in_flight`, `applied`. A worker stream promotes `desired` → `in_flight`, and
its `AckConfig` promotes `in_flight` → `applied`. Derivation is convergent: a
server only switches destination once the target actually serves it, so no
revision drops traffic mid-rollout.

## Images

| Image | Dockerfile | Base | Tagged |
|---|---|---|---|
| `ghcr.io/<owner>/guru-master` | `master.Dockerfile` | `distroless/cc-debian13:nonroot` | `latest` + `sha-<short>` on any `main` commit (merged PR or direct push); the tag name on every pushed tag, plus `<version>` for `master-v*` / `v*` |
| `ghcr.io/<owner>/guru-frontend` | `frontend.Dockerfile` | `distroless/nodejs24-debian13:nonroot` | `latest` + `sha-<short>` on any `main` commit (merged PR or direct push); the tag name on every pushed tag, plus `<version>` for `frontend-v*` / `v*` |

Every pushed tag builds both images and tags them with the tag name; the
`master-v*` / `frontend-v*` / `v*` conventions additionally produce a bare
`<version>` tag on the matching image.

Both build from the repository root — the frontend needs the whole Bun workspace
for the `app-protobuf` package:

```sh
docker build -f master.Dockerfile   -t guru-master   .
docker build -f frontend.Dockerfile -t guru-frontend .
```

`guru-master` is configured entirely through the environment (`GURU_WORKER_MODE`
selects the mode; `SURREALDB_NAMESPACE`, `SURREALDB_NAME` and `AMQP_URI` have no
defaults). The frontend listens on `:3000` and reaches the control plane through
`GURU_GRPC_URL`.

## Stack

Rust 2024 on Tokio, [`wakuwaku`](https://crates.io/crates/wakuwaku) +
[`kanau`](https://crates.io/crates/kanau) (everything is a `Processor`), gRPC via
Tonic, SurrealDB for storage (schema in `database/`, managed with surrealkit),
Redis for caching, AMQP for inter-module events, OpenTelemetry for tracing, and a
Bun workspace under `typescript/` sharing one generated API client.

Read [`AGENTS.md`](AGENTS.md) before adding code — it describes exactly how each
layer is organised. [`TEMPLATE_README.md`](TEMPLATE_README.md) documents the
upstream template this workspace started from.
