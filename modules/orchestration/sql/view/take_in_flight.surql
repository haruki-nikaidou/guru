BEGIN TRANSACTION;
LET $owner = (SELECT refresh_key_generation, watch_epoch FROM ONLY $server);
LET $taken = (UPDATE orchestration_server_config_view SET in_flight = desired
    WHERE server = $server AND in_flight = NONE AND desired != NONE
      AND desired.revision != applied.revision AND desired.revision != failed_revision
      AND $owner.refresh_key_generation = $generation AND $owner.watch_epoch = $epoch
    RETURN VALUE in_flight);
RETURN $taken[0];
COMMIT TRANSACTION;
