-- A colony of dens some of whose nests are broken, by the number the
-- world gives it: which are broken and since when, as one JSON. A colony
-- with every nest standing has no row.
CREATE TABLE colony (
    q     INTEGER NOT NULL,
    r     INTEGER NOT NULL,
    state JSONB   NOT NULL,
    PRIMARY KEY (q, r)
);

CREATE TRIGGER journal AFTER INSERT OR UPDATE OR DELETE ON colony FOR EACH ROW EXECUTE FUNCTION journal();
