-- `$canvas` is always the root of the derived tree.
-- Statement 0 is BEGIN, 1 the LET, 2 the IF; the RETURN is statement 3.
BEGIN TRANSACTION;
-- `generation` fences against edits made since the pass read the canvas;
-- `derived_generation < $generation` fences against a second pass that read the
-- same generation, so only the first one to arrive publishes its revisions.
LET $fenced = (UPDATE $canvas SET derived_generation = $generation
    WHERE generation = $generation AND derived_generation < $generation
    RETURN AFTER);
IF array::len($fenced) > 0 {
    FOR $u IN $updates {
        -- `invalid_pods` is written on every branch: a pod breaking or being fixed
        -- must be visible even when the served config is byte-identical.
        IF $u.desired != NONE {
            UPDATE orchestration_server_config_view
                SET desired = $u.desired, derive_error = $u.derive_error,
                    invalid_pods = $u.invalid_pods, waiting_for = $u.waiting_for
                WHERE server = $u.server;
        } ELSE {
            UPDATE orchestration_server_config_view
                SET derive_error = $u.derive_error, invalid_pods = $u.invalid_pods,
                    waiting_for = $u.waiting_for
                WHERE server = $u.server;
        };
        IF $u.clear_failure {
            UPDATE orchestration_server_config_view
                SET failed_revision = NONE, apply_error = NONE
                WHERE server = $u.server
        };
    };
    -- Subcanvas rows carry no generation of their own; keep them level so a
    -- canvas that was imported mid-edit can never look stale on its own.
    UPDATE orchestration_canvas SET derived_generation = generation
        WHERE id IN fn::orchestration_tree($canvas, 0) AND id != $canvas;
};
RETURN array::len($fenced) > 0;
COMMIT TRANSACTION;
