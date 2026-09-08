BEGIN TRANSACTION;
LET $ports = (SELECT VALUE id FROM orchestration_port WHERE owner = $id);
UPDATE orchestration_node SET retired_rev = $revision
    WHERE id = $id AND retired_rev IS NONE;
UPDATE orchestration_edge_connection SET retired_rev = $revision
    WHERE retired_rev IS NONE AND (in IN $ports OR out IN $ports);
COMMIT TRANSACTION;
