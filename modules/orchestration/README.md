# `orchestration` — canvases, servers, nodes and worker rollout

The control plane of the proxy fabric. Operators build a **canvas** of servers
and nodes; this module validates the topology, derives one `guru-worker` config
per server, and streams every new revision to the workers that registered for it.

## Layout

```
src/
├── lib.rs          # crate root: declares the modules below
├── config.rs       # strongly typed module configuration (DB-backed, Redis-cached)
├── utils/ids.rs    # record id ↔ wire string conversion
├── entities/
│   └── surreal/    # canvas, server, node, port, connection, revision, topology
├── services/       # CRUD, topology rules, config derivation, rollout, agent, watch
├── hooks/gc.rs     # the RCU garbage collector
└── rpc/            # the operator API and the worker API, plus refresh-key middleware
```

## Node kinds

A canvas is a bipartite dataflow over two independent port kinds:

- **DeriveListen** flows `Pod(out) → Entry|Relay(in)`: what a pod's listener looks
  like on the wire.
- **DeriveDestination** flows `Exit(out) → … → Pod(in)`: where a pod's traffic goes,
  possibly through load balancers and relays.

Every port carries at most one edge. `CanvasImport`/`CanvasExport` are rejected
until subcanvases land.

## RCU

Nodes and edges are never mutated in place. A spec change writes a replacement row
and stamps the old one with the global revision that retired it
(`DEFINE SEQUENCE orchestration_revision`). Each server keeps
`orchestration_server_config_revision` rows for the configs it might still be
running; the collector in `hooks::gc` deletes a retired row only once no retained
revision references it. `ForceDeleteNode`/`ForceDisconnect` (Admin only) bypass
this and delete immediately.

## Rollout

```
mutation ─► validate projected topology ─► write rows ─► stamp_canvas
                                                            │
                                     derive per server ─────┴─► revision row + desired_revision
                                                                        │
worker: Register ─► WatchConfig (stream) ─► apply ─► AckConfig ─────────┘
```

A worker registers with an **operator API key** and receives a dynamic refresh key
that it keeps in memory only; the master stores its SHA-256 digest. Re-registering
rotates the key, which kills the previous session's stream — that is how a master
learns a worker restarted. Only servers whose derived TOML actually changed get a
new revision, so unrelated servers never restart their listeners.

## Dependency direction

- `rpc` depends on `services` (and `rpguru_sdk`); the watch hub lives in
  `services::watch` so nothing below the edge depends on the edge.
- `services` depend on `entities` and `config`; every query is a `Processor` in
  `entities/surreal`.
- The schema lives in `database/schema/orchestration.surql`; the integration tests
  apply that exact file to a `mem://` database.

See `AGENTS.md` at the workspace root for the full authoring guide.
