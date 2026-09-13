---
title: Rollout Model
description: How a canvas edit becomes a config revision applied by a worker.
---

## From edit to applied config

Mutations bump the canvas generation and publish `CanvasDirty`. The derivation hook re-derives the
whole canvas; a cron sweep catches anything a lost message missed.

Every server has **one config view** holding three snapshots — `desired`, `in_flight` and
`applied`. A worker stream promotes `desired` → `in_flight`, and its `AckConfig` promotes
`in_flight` → `applied`.

```text
canvas mutation ──▶ CanvasDirty ──┐
                                  ├──▶ derivation hook
    cron: stale-canvas sweep ─────┘
                                       │
                                       ▼
                             config view: desired
                                       │ worker stream
                                       ▼
                                   in_flight
                                       │ AckConfig
                                       ▼
                                    applied
```

## Convergence

Derivation is convergent: a server only switches destination once the target actually serves it, so
no revision drops traffic mid-rollout. That property is why the three-snapshot view exists instead
of a single "current config" field — the master always knows what a worker has really applied.

## Inspecting a derived config

`manage-tool` prints the exact TOML the master derived for one server:

```sh
cargo run -p manage-tool -- \
  --address ws://127.0.0.1:8000 --username root --password root \
  --namespace guru --database guru \
  orchestration export-config --server <orchestration_server key>
```

The same model backs standalone workers: the printed file can be handed to
`guru-worker --config`.

## Forwarding shapes

Each forwarding pairs a listener with a destination:

- **Listener** — `raw`, `tls`, or an inbound relay.
- **Destination** — a direct **exit**, a **relay** to another node over TLS-over-TCP or QUIC, or a
  **load-balance** group.

PROXY protocol v1 and v2 are supported on both ends.
