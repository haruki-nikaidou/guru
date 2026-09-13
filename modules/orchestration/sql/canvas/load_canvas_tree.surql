BEGIN TRANSACTION;
LET $root = fn::orchestration_root($canvas);
LET $tree = fn::orchestration_tree($root, 0);
SELECT * FROM orchestration_canvas WHERE id IN $tree;
SELECT canvas, spec.config.canvas AS target FROM orchestration_node
    WHERE canvas IN $tree AND spec.config.canvas != NONE;
RETURN $root;
COMMIT TRANSACTION;
