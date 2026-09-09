BEGIN TRANSACTION;
UPDATE orchestration_server_config_view
    SET applied = NONE, in_flight = NONE, apply_error = NONE, failed_revision = NONE
    WHERE server = $server;
UPDATE $canvas SET generation += 1;
COMMIT TRANSACTION;
