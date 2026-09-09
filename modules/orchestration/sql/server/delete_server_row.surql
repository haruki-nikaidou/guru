BEGIN TRANSACTION;
DELETE orchestration_server_config_view WHERE server = $id;
DELETE server_ip_record WHERE server = $id;
DELETE $id;
UPDATE $canvas SET generation += 1;
COMMIT TRANSACTION;
