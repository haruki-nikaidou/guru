BEGIN TRANSACTION;
LET $ok = IF $error = NONE {
    UPDATE orchestration_server_config_view
        SET applied = in_flight, in_flight = NONE, apply_error = NONE, failed_revision = NONE
        WHERE server = $server AND in_flight.revision = $revision RETURN AFTER
} ELSE {
    UPDATE orchestration_server_config_view
        SET in_flight = NONE, apply_error = $error, failed_revision = $revision
        WHERE server = $server AND in_flight.revision = $revision RETURN AFTER
};
IF array::len($ok) > 0 { UPDATE $canvas SET generation += 1 };
RETURN array::len($ok) > 0;
COMMIT TRANSACTION;
