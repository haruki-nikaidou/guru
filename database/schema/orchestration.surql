-- Orchestration module schema.
--
-- Single source of truth for the `orchestration` module's tables. Applied in
-- development with `surrealkit sync`; the module's integration tests apply this
-- exact file against an in-memory database. Nodes and edges are RCU rows: a spec
-- change writes a replacement row and stamps `retired_rev` on the old one, which
-- is deleted only once no retained `orchestration_server_config_revision` row
-- references it. `orchestration_revision` is the global revision counter.

DEFINE SEQUENCE orchestration_revision;

DEFINE TABLE orchestration_canvas SCHEMAFULL;
DEFINE FIELD name ON orchestration_canvas TYPE string;
DEFINE FIELD description ON orchestration_canvas TYPE string;

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
DEFINE FIELD desired_revision ON orchestration_server TYPE int DEFAULT 0;
DEFINE FIELD applied_revision ON orchestration_server TYPE int DEFAULT 0;
DEFINE FIELD last_apply_error ON orchestration_server TYPE option<string>;
DEFINE FIELD current_dynamic_refresh_key ON orchestration_server TYPE option<string>;
DEFINE FIELD refresh_key_generation ON orchestration_server TYPE int DEFAULT 0;
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
DEFINE FIELD position ON orchestration_node TYPE object;
DEFINE FIELD position.x ON orchestration_node TYPE int;
DEFINE FIELD position.y ON orchestration_node TYPE int;
DEFINE FIELD created_rev ON orchestration_node TYPE int;
DEFINE FIELD retired_rev ON orchestration_node TYPE option<int>;
DEFINE FIELD replaces ON orchestration_node TYPE option<record<orchestration_node>>;
DEFINE INDEX orchestration_node_canvas ON orchestration_node FIELDS canvas, retired_rev;

DEFINE TABLE orchestration_port SCHEMAFULL;
DEFINE FIELD owner ON orchestration_port TYPE record<orchestration_node>;
DEFINE FIELD kind ON orchestration_port TYPE string ASSERT $value INSIDE ["derive_listen", "derive_destination"];
DEFINE FIELD direction ON orchestration_port TYPE string ASSERT $value INSIDE ["input", "output"];
DEFINE FIELD key ON orchestration_port TYPE string;
DEFINE FIELD position ON orchestration_port TYPE int;
DEFINE INDEX orchestration_port_owner ON orchestration_port FIELDS owner;

DEFINE TABLE orchestration_edge_connection TYPE RELATION IN orchestration_port OUT orchestration_port SCHEMAFULL;
DEFINE FIELD created_rev ON orchestration_edge_connection TYPE int;
DEFINE FIELD retired_rev ON orchestration_edge_connection TYPE option<int>;
DEFINE INDEX orchestration_edge_connection_in ON orchestration_edge_connection FIELDS in;
DEFINE INDEX orchestration_edge_connection_out ON orchestration_edge_connection FIELDS out;

DEFINE TABLE orchestration_server_config_revision SCHEMAFULL;
DEFINE FIELD server ON orchestration_server_config_revision TYPE record<orchestration_server>;
DEFINE FIELD revision ON orchestration_server_config_revision TYPE int;
DEFINE FIELD nodes ON orchestration_server_config_revision TYPE array<record<orchestration_node>>;
DEFINE FIELD edges ON orchestration_server_config_revision TYPE array<record<orchestration_edge_connection>>;
DEFINE FIELD toml ON orchestration_server_config_revision TYPE string;
DEFINE FIELD created_at ON orchestration_server_config_revision TYPE datetime DEFAULT time::now();
DEFINE INDEX orchestration_server_config_revision_key ON orchestration_server_config_revision FIELDS server, revision UNIQUE;
