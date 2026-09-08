BEGIN TRANSACTION;
LET $servers = (SELECT VALUE id FROM orchestration_server WHERE canvas = $id);
DELETE orchestration_server_config_revision WHERE server IN $servers;
DELETE server_ip_record WHERE server IN $servers;
DELETE orchestration_server WHERE id IN $servers;
DELETE $id;
COMMIT TRANSACTION;
