BEGIN TRANSACTION;
LET $node = (CREATE ONLY orchestration_node CONTENT {
    canvas: $canvas, name: $name, comment: $comment, spec: $spec,
    position: $position, created_rev: $created_rev, retired_rev: NONE,
    replaces: $replaces
});
LET $ports = (INSERT INTO orchestration_port
    (SELECT $node.id AS owner, kind, direction, key, position FROM $new_ports)
    RETURN AFTER);
RETURN { node: $node, ports: $ports };
COMMIT TRANSACTION;
