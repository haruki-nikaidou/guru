BEGIN TRANSACTION;
LET $node = (CREATE ONLY orchestration_node CONTENT {
    canvas: $canvas, name: $name, comment: $comment, spec: $spec,
    position: $position
});
LET $ports = (INSERT INTO orchestration_port
    (SELECT $node.id AS owner, kind, direction, key, position FROM $new_ports)
    RETURN AFTER);
UPDATE $canvas SET generation += 1;
RETURN { node: $node, ports: $ports };
COMMIT TRANSACTION;
