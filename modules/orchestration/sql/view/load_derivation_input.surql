-- One consistent read of everything a derivation pass needs.
-- Statement order is load-bearing: `LoadCanvasDerivationInput` takes results by index.
BEGIN TRANSACTION;
SELECT generation, derived_generation FROM ONLY $canvas;
SELECT * FROM orchestration_server WHERE canvas = $canvas;
SELECT * FROM server_ip_record WHERE server.canvas = $canvas;
SELECT * FROM orchestration_node WHERE canvas = $canvas;
SELECT * FROM orchestration_port WHERE owner.canvas = $canvas ORDER BY position;
SELECT * FROM orchestration_edge_connection WHERE in.owner.canvas = $canvas;
SELECT * FROM orchestration_server_config_view WHERE server.canvas = $canvas;
COMMIT TRANSACTION;
