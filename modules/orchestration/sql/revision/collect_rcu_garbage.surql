BEGIN TRANSACTION;
LET $nodes_ref = array::flatten((SELECT VALUE nodes FROM orchestration_server_config_revision));
LET $edges_ref = array::flatten((SELECT VALUE edges FROM orchestration_server_config_revision));
LET $dead_nodes = (SELECT VALUE id FROM orchestration_node
    WHERE retired_rev IS NOT NONE AND id NOT IN $nodes_ref);
LET $dead_ports = (SELECT VALUE id FROM orchestration_port WHERE owner IN $dead_nodes);
LET $dead_edges = (SELECT VALUE id FROM orchestration_edge_connection
    WHERE (retired_rev IS NOT NONE AND id NOT IN $edges_ref)
       OR in IN $dead_ports OR out IN $dead_ports);
DELETE orchestration_edge_connection WHERE id IN $dead_edges;
DELETE orchestration_port WHERE id IN $dead_ports;
DELETE orchestration_node WHERE id IN $dead_nodes;
RETURN { nodes: array::len($dead_nodes), edges: array::len($dead_edges) };
COMMIT TRANSACTION;
