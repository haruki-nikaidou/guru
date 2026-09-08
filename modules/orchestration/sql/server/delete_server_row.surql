BEGIN TRANSACTION;
DELETE orchestration_server_config_revision WHERE server = $id;
DELETE server_ip_record WHERE server = $id;
DELETE $id;
COMMIT TRANSACTION;
