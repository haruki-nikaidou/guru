BEGIN TRANSACTION;
LET $node = (CREATE ONLY orchestration_node CONTENT {
    canvas: $canvas, name: $name, comment: $comment, spec: $spec,
    position: $position, created_rev: $revision, retired_rev: NONE,
    replaces: $old
});
LET $ports = (INSERT INTO orchestration_port
    (SELECT $node.id AS owner, kind, direction, key, position FROM $new_ports)
    RETURN AFTER);
LET $old_ports = (SELECT VALUE id FROM orchestration_port WHERE owner = $old);
UPDATE orchestration_node SET retired_rev = $revision
    WHERE id = $old AND retired_rev IS NONE;
UPDATE orchestration_edge_connection SET retired_rev = $revision
    WHERE retired_rev IS NONE AND (in IN $old_ports OR out IN $old_ports);
FOR $c IN $carry {
    LET $np = (SELECT VALUE id FROM $ports WHERE key = $c.new_port_key)[0];
    LET $other = $c.other_port;
    IF $c.new_port_is_source {
        RELATE $np->orchestration_edge_connection->$other
            CONTENT { created_rev: $revision, retired_rev: NONE };
    } ELSE {
        RELATE $other->orchestration_edge_connection->$np
            CONTENT { created_rev: $revision, retired_rev: NONE };
    };
};
RETURN { node: $node, ports: $ports };
COMMIT TRANSACTION;
