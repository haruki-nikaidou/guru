BEGIN TRANSACTION;
LET $rotated = (UPDATE $server SET current_dynamic_refresh_key = $digest,
    refresh_key_generation += 1, session_lease_until = $lease_until, last_seen_at = $now
    WHERE session_lease_until = NONE OR session_lease_until <= $now
    RETURN AFTER);
IF array::len($rotated) > 0 {
    LET $v = (SELECT * FROM ONLY orchestration_server_config_view WHERE server = $server LIMIT 1);
    IF $running_revision > 0 {
        IF $v.desired.revision = $running_revision {
            UPDATE $v.id SET applied = desired, apply_error = NONE, failed_revision = NONE;
        } ELSE IF $v.in_flight.revision = $running_revision {
            UPDATE $v.id SET applied = in_flight, apply_error = NONE, failed_revision = NONE;
        } ELSE IF $v.applied.revision != $running_revision {
            UPDATE $v.id SET apply_error = "running revision " + <string>$running_revision + " unknown";
        };
    };
    UPDATE $v.id SET in_flight = NONE;
    UPDATE $canvas SET generation += 1;
};
RETURN $rotated[0];
COMMIT TRANSACTION;
