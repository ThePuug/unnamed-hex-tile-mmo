-- A den is kept by its site's tile and a colony by the nests of its sites.
-- The sites are where each one's own ground takes a den, of a colony's
-- candidates, where they were a clump's few: every den kept before stands
-- on a tile that is a site no more, and none is kept. The change is
-- stamped at the last moment of the calendar the journal holds.
SELECT set_config('mmo.when', (SELECT COALESCE(MAX(wall), 0) FROM history)::TEXT, TRUE);
DELETE FROM den;
DELETE FROM colony;
