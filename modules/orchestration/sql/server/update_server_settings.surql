BEGIN TRANSACTION;
UPDATE $id SET name = $name, icon = $icon, comment = $comment,
    ipv6_resolve = $ipv6_resolve, log_level = $log_level RETURN AFTER;
fn::orchestration_touch($canvas);
COMMIT TRANSACTION;
