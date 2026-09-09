BEGIN TRANSACTION;
LET $ports = (SELECT VALUE id FROM orchestration_port WHERE owner = $id);
DELETE orchestration_edge_connection WHERE in IN $ports OR out IN $ports;
DELETE orchestration_port WHERE id IN $ports;
DELETE $id;
UPDATE $canvas SET generation += 1;
COMMIT TRANSACTION;
