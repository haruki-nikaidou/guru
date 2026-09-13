-- Deleting an ip record a pod still listens on would leave that pod pointing at
-- nothing, and a pod with no ip record cannot be attributed to a server at all:
-- its derivation failure could then be neither published nor reported. The
-- `spec.config.ip` field assertion cannot catch this, because a field assertion
-- is not re-evaluated when the record it points at is deleted, so the check has
-- to live in the same transaction as the delete. A service-layer pre-check runs
-- in an earlier transaction and races with a concurrent node write.
BEGIN TRANSACTION;
LET $used = (SELECT VALUE id FROM orchestration_node WHERE spec.config.ip = $id);
IF array::len($used) > 0 {
    THROW "ip record is still used by " + <string> array::len($used) + " pod(s)";
};
DELETE $id;
fn::orchestration_touch($canvas);
COMMIT TRANSACTION;
