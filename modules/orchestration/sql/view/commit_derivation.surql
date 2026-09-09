BEGIN TRANSACTION;
LET $fenced = (UPDATE $canvas SET derived_generation = $generation WHERE generation = $generation RETURN AFTER);
IF array::len($fenced) > 0 {
    FOR $u IN $updates {
        IF $u.desired != NONE {
            UPDATE orchestration_server_config_view
                SET desired = $u.desired, derive_error = $u.derive_error, waiting_for = $u.waiting_for
                WHERE server = $u.server;
        } ELSE {
            UPDATE orchestration_server_config_view
                SET derive_error = $u.derive_error, waiting_for = $u.waiting_for
                WHERE server = $u.server;
        };
        IF $u.clear_failure {
            UPDATE orchestration_server_config_view
                SET failed_revision = NONE, apply_error = NONE
                WHERE server = $u.server
        };
    };
};
RETURN array::len($fenced) > 0;
COMMIT TRANSACTION;
