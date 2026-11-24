-- Every change to what the store keeps of the world, journaled as it is
-- made, so the world can be rolled back to an earlier moment: the row as
-- it was, none where it was not there, and as it became, none where it
-- is gone; stamped on the real clock and on the calendar, which whoever
-- writes says with `SET LOCAL mmo.when`. A change that does not say is
-- refused, so nothing kept changes outside the journal. Who owns a chunk
-- or an account, and which player an identity names, are not journaled:
-- a rollback moves no ownership and forgets no player.
CREATE TABLE history (
    id     BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    at     TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    wall   BIGINT      NOT NULL,
    tbl    TEXT        NOT NULL,
    was    JSONB,
    became JSONB,
    -- Rolled back: kept, so what was undone can still be read, and never
    -- undone twice.
    undone BOOLEAN     NOT NULL DEFAULT FALSE
);

CREATE INDEX history_at ON history (at) WHERE NOT undone;
CREATE INDEX history_wall ON history (wall) WHERE NOT undone;

CREATE FUNCTION journal() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    -- A rollback's own changes are not journaled
    IF current_setting('mmo.rolling', TRUE) = 'on' THEN
        RETURN NULL;
    END IF;
    IF COALESCE(current_setting('mmo.when', TRUE), '') = '' THEN
        RAISE EXCEPTION 'a change to % names no moment of the calendar: SET LOCAL mmo.when first', TG_TABLE_NAME;
    END IF;
    INSERT INTO history (wall, tbl, was, became) VALUES (
        current_setting('mmo.when')::BIGINT,
        TG_TABLE_NAME,
        CASE WHEN TG_OP <> 'INSERT' THEN to_jsonb(OLD) END,
        CASE WHEN TG_OP <> 'DELETE' THEN to_jsonb(NEW) END
    );
    RETURN NULL;
END $$;

CREATE TRIGGER journal AFTER INSERT OR UPDATE OR DELETE ON tile_cover FOR EACH ROW EXECUTE FUNCTION journal();
CREATE TRIGGER journal AFTER INSERT OR UPDATE OR DELETE ON pile FOR EACH ROW EXECUTE FUNCTION journal();
CREATE TRIGGER journal AFTER INSERT OR UPDATE OR DELETE ON construct FOR EACH ROW EXECUTE FUNCTION journal();
CREATE TRIGGER journal AFTER INSERT OR UPDATE OR DELETE ON den FOR EACH ROW EXECUTE FUNCTION journal();
CREATE TRIGGER journal AFTER INSERT OR UPDATE OR DELETE ON camp FOR EACH ROW EXECUTE FUNCTION journal();
CREATE TRIGGER journal AFTER INSERT OR UPDATE OR DELETE ON hub FOR EACH ROW EXECUTE FUNCTION journal();
CREATE TRIGGER journal AFTER INSERT OR UPDATE OR DELETE ON player_character FOR EACH ROW EXECUTE FUNCTION journal();
CREATE TRIGGER journal AFTER INSERT OR UPDATE OR DELETE ON calendar FOR EACH ROW EXECUTE FUNCTION journal();
CREATE TRIGGER journal AFTER INSERT OR UPDATE OR DELETE ON ledger FOR EACH ROW EXECUTE FUNCTION journal();

-- Undoes every journaled change made after `since_at` on the real clock,
-- or after `since_wall` on the calendar, whichever is given, newest
-- first: the row as it became is taken out and the row as it was put
-- back, each found by its table's primary key. Returns how many it
-- undid. The ledger's entries made since go with the moves they
-- recorded, and stay readable in the journal.
CREATE FUNCTION roll_back(since_at TIMESTAMPTZ, since_wall BIGINT) RETURNS BIGINT LANGUAGE plpgsql AS $$
DECLARE
    change RECORD;
    same   TEXT;
    whole  TEXT;
    undid  BIGINT := 0;
BEGIN
    PERFORM set_config('mmo.rolling', 'on', TRUE);
    FOR change IN
        SELECT * FROM history
        WHERE NOT undone AND ((since_at IS NOT NULL AND at > since_at) OR (since_wall IS NOT NULL AND wall > since_wall))
        ORDER BY id DESC
    LOOP
        IF change.became IS NOT NULL THEN
            SELECT string_agg(format('t.%I = r.%I', a.attname, a.attname), ' AND ') INTO same
            FROM pg_index i JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY (i.indkey)
            WHERE i.indrelid = change.tbl::REGCLASS AND i.indisprimary;
            EXECUTE format('DELETE FROM %I t USING jsonb_populate_record(NULL::%I, $1) r WHERE %s', change.tbl, change.tbl, same) USING change.became;
        END IF;
        IF change.was IS NOT NULL THEN
            -- A JSON null kept in a column comes back from the journal as
            -- no value at all: where the column takes none, it was the null
            SELECT string_agg(
                CASE WHEN a.attnotnull AND a.atttypid = 'jsonb'::REGTYPE THEN format('COALESCE(r.%I, ''null''::JSONB)', a.attname) ELSE format('r.%I', a.attname) END,
                ', ' ORDER BY a.attnum
            ) INTO whole
            FROM pg_attribute a
            WHERE a.attrelid = change.tbl::REGCLASS AND a.attnum > 0 AND NOT a.attisdropped;
            EXECUTE format('INSERT INTO %I OVERRIDING SYSTEM VALUE SELECT %s FROM jsonb_populate_record(NULL::%I, $1) r', change.tbl, whole, change.tbl) USING change.was;
        END IF;
        UPDATE history SET undone = TRUE WHERE id = change.id;
        undid := undid + 1;
    END LOOP;
    PERFORM set_config('mmo.rolling', 'off', TRUE);
    RETURN undid;
END $$;
