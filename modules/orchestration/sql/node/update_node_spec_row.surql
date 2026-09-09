BEGIN TRANSACTION;
UPDATE $id SET spec = $spec;
LET $keep = (SELECT VALUE key FROM $new_ports);
DELETE orchestration_port WHERE owner = $id AND key NOT IN $keep;
FOR $p IN $new_ports {
    LET $existing = (SELECT VALUE id FROM orchestration_port WHERE owner = $id AND key = $p.key);
    IF array::len($existing) > 0 {
        UPDATE $existing[0] SET kind = $p.kind, direction = $p.direction, position = $p.position;
    } ELSE {
        CREATE orchestration_port CONTENT {
            owner: $id, kind: $p.kind, direction: $p.direction, key: $p.key, position: $p.position
        };
    };
};
UPDATE $canvas SET generation += 1;
RETURN {
    node: (SELECT * FROM ONLY $id),
    ports: (SELECT * FROM orchestration_port WHERE owner = $id ORDER BY position)
};
COMMIT TRANSACTION;
