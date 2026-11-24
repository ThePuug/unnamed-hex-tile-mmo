-- A map is one of a kind, and known by its number. A server takes a run
-- of numbers at a time and gives each map it makes the next of them, so
-- no two servers give one number twice. A number is never given again:
-- a sequence is no table, and a rollback leaves it where it stands.
CREATE SEQUENCE map_number AS BIGINT START WITH 1024 INCREMENT BY 1024;

-- What a map has charted: each chunk as it lay when the map was last
-- carried past it, every tile's level and whether it was full
-- (`common_bevy::chart::Leaf::kept`). Map 0 is no player's: it is the
-- ground as it lies now, of every chunk any map was charted from, which
-- a way found on a map is tried against.
CREATE TABLE chart_leaf (
    map   BIGINT NOT NULL,
    chunk BIGINT NOT NULL,
    tiles BYTEA  NOT NULL,
    PRIMARY KEY (map, chunk)
);

CREATE TRIGGER journal AFTER INSERT OR UPDATE OR DELETE ON chart_leaf FOR EACH ROW EXECUTE FUNCTION journal();
