-- Live **and** retiring rows, for rendering a rollout in flight.
-- Statement order is load-bearing: `load_canvas` takes results by index.
SELECT * FROM orchestration_server WHERE canvas = $canvas;
SELECT * FROM server_ip_record WHERE server.canvas = $canvas;
SELECT * FROM orchestration_node WHERE canvas = $canvas;
SELECT * FROM orchestration_port WHERE owner.canvas = $canvas ORDER BY position;
SELECT * FROM orchestration_edge_connection WHERE in.owner.canvas = $canvas;
