-- Statement 0 is BEGIN; the RETURN below is statement 5.
--
-- The node name is the forwarding tag in the derived worker config, so a rename
-- has to schedule a derivation; a move or a comment edit is invisible to workers
-- and leaves the generation alone. The old name is read inside this transaction
-- rather than by the caller, so a metadata write that started before a
-- concurrent rename cannot overwrite that rename without bumping.
BEGIN TRANSACTION;
LET $renamed = (SELECT VALUE name FROM ONLY $id) != $name;
UPDATE $id SET name = $name, comment = $comment;
IF $position != NONE {
    UPDATE $id SET position = $position;
};
IF $renamed {
    UPDATE $canvas SET generation += 1;
};
RETURN {
    renamed: $renamed,
    node: (SELECT * FROM ONLY $id),
    ports: (SELECT * FROM orchestration_port WHERE owner = $id ORDER BY position)
};
COMMIT TRANSACTION;
