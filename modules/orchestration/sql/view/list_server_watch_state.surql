SELECT server AS id, server.refresh_key_generation AS refresh_key_generation,
    server.watch_epoch AS watch_epoch, desired.revision AS desired_revision,
    in_flight.revision AS in_flight_revision,
    applied.revision AS applied_revision, failed_revision
FROM orchestration_server_config_view WHERE server IN $servers;
