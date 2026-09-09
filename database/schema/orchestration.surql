-- Orchestration module schema.
--
-- Single source of truth for the `orchestration` module's tables. Applied in
-- development with `surrealkit sync`; the module's integration tests apply this
-- exact file against an in-memory database. Nodes, ports and edges are edited in
-- place; what a worker runs lives in `orchestration_server_config_view`, one row
-- per server holding three immutable snapshots (`desired`, `in_flight`,
-- `applied`). Derivation is fenced by the canvas generation counter: every
-- mutating transaction bumps `generation`, and a derivation commits only while
-- `generation` still equals the value it read, stamping `derived_generation`.

DEFINE TABLE orchestration_canvas SCHEMAFULL;
DEFINE FIELD name ON orchestration_canvas TYPE string;
DEFINE FIELD description ON orchestration_canvas TYPE string;
DEFINE FIELD generation ON orchestration_canvas TYPE int DEFAULT 0;
DEFINE FIELD derived_generation ON orchestration_canvas TYPE int DEFAULT 0;

DEFINE TABLE orchestration_server SCHEMAFULL;
DEFINE FIELD canvas ON orchestration_server TYPE record<orchestration_canvas>;
DEFINE FIELD name ON orchestration_server TYPE string;
DEFINE FIELD icon ON orchestration_server TYPE string;
DEFINE FIELD comment ON orchestration_server TYPE string;
DEFINE FIELD position ON orchestration_server TYPE object;
DEFINE FIELD position.x ON orchestration_server TYPE int;
DEFINE FIELD position.y ON orchestration_server TYPE int;
DEFINE FIELD ipv6_resolve ON orchestration_server TYPE string ASSERT $value INSIDE ["required", "preferred", "tolerated", "forbidden"];
DEFINE FIELD log_level ON orchestration_server TYPE string;
DEFINE FIELD current_dynamic_refresh_key ON orchestration_server TYPE option<string>;
DEFINE FIELD refresh_key_generation ON orchestration_server TYPE int DEFAULT 0;
DEFINE FIELD watch_epoch ON orchestration_server TYPE int DEFAULT 0;
DEFINE FIELD session_lease_until ON orchestration_server TYPE option<datetime>;
DEFINE FIELD last_seen_at ON orchestration_server TYPE option<datetime>;
DEFINE INDEX orchestration_server_canvas ON orchestration_server FIELDS canvas;
DEFINE INDEX orchestration_server_refresh_key ON orchestration_server FIELDS current_dynamic_refresh_key;

DEFINE TABLE server_ip_record SCHEMAFULL;
DEFINE FIELD server ON server_ip_record TYPE record<orchestration_server>;
DEFINE FIELD ip ON server_ip_record TYPE string;
DEFINE FIELD country ON server_ip_record TYPE string;
DEFINE INDEX server_ip_record_server ON server_ip_record FIELDS server;

DEFINE TABLE orchestration_node SCHEMAFULL;
DEFINE FIELD canvas ON orchestration_node TYPE record<orchestration_canvas>;
DEFINE FIELD name ON orchestration_node TYPE string;
DEFINE FIELD comment ON orchestration_node TYPE string;
DEFINE FIELD spec ON orchestration_node TYPE object FLEXIBLE;
-- A pod's spec carries `ip`, the `server_ip_record` it listens on, and that link
-- is what attributes the pod to a server: derivation resolves the pod's server
-- through it, so a per-pod failure can be recorded on that server's
-- `orchestration_server_config_view` row. A dangling link has no server to
-- attribute it to, making it the one derivation failure that can be neither
-- published nor reported, so the database refuses to store one. The same applies
-- to an ip record in a different canvas: derivation loads one canvas at a time,
-- so a foreign link is indistinguishable from a missing one.
--
-- The path is `spec.config.ip`, not `spec.ip`: `NodeSpec` is a tagged enum
-- (`#[surreal(tag = "type", content = "config")]`), so a pod row reads
-- `spec: { type: "pod", config: { ip, port } }`. `spec` stays FLEXIBLE — an
-- Entry, Exit or Relay carries a different `config` shape with no `ip` key at
-- all, hence `option<...>` and the explicit `NONE` arm in the assertion.
DEFINE FIELD spec.config.ip ON orchestration_node TYPE option<record<server_ip_record>>
    ASSERT $value == NONE
        OR (record::exists($value) AND $value.server.canvas = $this.canvas);
DEFINE FIELD position ON orchestration_node TYPE object;
DEFINE FIELD position.x ON orchestration_node TYPE int;
DEFINE FIELD position.y ON orchestration_node TYPE int;
DEFINE INDEX orchestration_node_canvas ON orchestration_node FIELDS canvas;

DEFINE TABLE orchestration_port SCHEMAFULL;
DEFINE FIELD owner ON orchestration_port TYPE record<orchestration_node>;
DEFINE FIELD kind ON orchestration_port TYPE string ASSERT $value INSIDE ["derive_listen", "derive_destination"];
DEFINE FIELD direction ON orchestration_port TYPE string ASSERT $value INSIDE ["input", "output"];
DEFINE FIELD key ON orchestration_port TYPE string;
DEFINE FIELD position ON orchestration_port TYPE int;
DEFINE INDEX orchestration_port_owner ON orchestration_port FIELDS owner;

DEFINE TABLE orchestration_edge_connection TYPE RELATION IN orchestration_port OUT orchestration_port SCHEMAFULL;
-- A port carries at most one edge; the database, not only the topology checker,
-- enforces it, so two concurrent connects cannot both persist an edge on one port.
DEFINE INDEX orchestration_edge_connection_in ON orchestration_edge_connection FIELDS in UNIQUE;
DEFINE INDEX orchestration_edge_connection_out ON orchestration_edge_connection FIELDS out UNIQUE;

DEFINE TABLE orchestration_server_config_view SCHEMAFULL;
DEFINE FIELD server ON orchestration_server_config_view TYPE record<orchestration_server>;
DEFINE FIELD desired ON orchestration_server_config_view TYPE option<object> FLEXIBLE;
DEFINE FIELD in_flight ON orchestration_server_config_view TYPE option<object> FLEXIBLE;
DEFINE FIELD applied ON orchestration_server_config_view TYPE option<object> FLEXIBLE;
DEFINE FIELD failed_revision ON orchestration_server_config_view TYPE option<int>;
DEFINE FIELD apply_error ON orchestration_server_config_view TYPE option<string>;
DEFINE FIELD derive_error ON orchestration_server_config_view TYPE option<string>;
-- `derive_error` is a server-level failure: the server produced no config at all.
-- `invalid_pods` is the per-pod counterpart: these pods could not be derived
-- while the rest of this server's config stayed valid and was published, so an
-- operator can see exactly which pod is broken and why. Each element is
-- `{ node, pod, listen, error }`; FLEXIBLE because the elements are objects and
-- a SCHEMAFULL table otherwise rejects their keys. A server-level failure leaves
-- this list empty.
DEFINE FIELD invalid_pods ON orchestration_server_config_view TYPE array<object> FLEXIBLE DEFAULT [];
DEFINE FIELD waiting_for ON orchestration_server_config_view TYPE array<record<orchestration_server>> DEFAULT [];
DEFINE INDEX orchestration_server_config_view_server ON orchestration_server_config_view FIELDS server UNIQUE;
