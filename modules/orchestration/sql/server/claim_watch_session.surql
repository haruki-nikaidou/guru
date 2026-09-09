BEGIN TRANSACTION;
LET $claimed = (UPDATE $server SET watch_epoch += 1, session_lease_until = $lease_until,
    last_seen_at = $now
    WHERE refresh_key_generation = $generation
    RETURN AFTER);
IF array::len($claimed) > 0 {
    UPDATE orchestration_server_config_view SET in_flight = NONE WHERE server = $server;
};
RETURN $claimed[0];
COMMIT TRANSACTION;
