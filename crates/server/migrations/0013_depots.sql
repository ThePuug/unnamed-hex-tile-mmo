-- What wagons have left at a hub, by the tile the hub was known by when
-- they came: its stacks, as a pile's are kept.
CREATE TABLE depot (
    q      INTEGER NOT NULL,
    r      INTEGER NOT NULL,
    stacks JSONB   NOT NULL,
    PRIMARY KEY (q, r)
);

CREATE TRIGGER journal AFTER INSERT OR UPDATE OR DELETE ON depot FOR EACH ROW EXECUTE FUNCTION journal();
