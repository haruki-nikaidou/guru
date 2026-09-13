-- Statement 0 is BEGIN, 1 the rename check, 2 the update, 3 the move, 4 the
-- import sync, 5 the conditional touch; the RETURN is statement 6.
--
-- The node name is the forwarding tag in the derived worker config, so a rename
-- has to schedule a derivation; a move or a comment edit is invisible to workers
-- and leaves the generation alone. The old name is read inside this transaction
-- rather than by the caller, so a metadata write that started before a
-- concurrent rename cannot overwrite that rename without bumping.
--
-- Moving an export node on the y axis renumbers the mirrored ports on the node
-- importing its canvas (`$import_sync`); that is a UI-only change too.
BEGIN TRANSACTION;
LET $renamed = (SELECT VALUE name FROM ONLY $id) != $name;
UPDATE $id SET name = $name, comment = $comment;
IF $position != NONE {
    UPDATE $id SET position = $position;
};
IF $import_sync != NONE {
    fn::orchestration_reshape_ports($import_sync.node, $import_sync.ports);
};
IF $renamed {
    fn::orchestration_touch($canvas);
};
RETURN {
    renamed: $renamed,
    node: (SELECT * FROM ONLY $id),
    ports: (SELECT * FROM orchestration_port WHERE owner = $id ORDER BY position)
};
COMMIT TRANSACTION;
