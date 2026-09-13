BEGIN TRANSACTION;
LET $edge = (RELATE ONLY $source->orchestration_edge_connection->$target);
fn::orchestration_touch($canvas);
RETURN $edge;
COMMIT TRANSACTION;
