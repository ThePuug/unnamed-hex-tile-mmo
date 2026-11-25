-- A hub is its build sites: the tile it is known by, each site as
-- `[q, r, what]`, what stands on it by its number, and the tiles of the
-- hubs it took in, together as one JSON. A hub kept before hubs had
-- sites holds its hearths alone, and is laid as a camp round the first
-- when it is read.
ALTER TABLE hub
    ALTER COLUMN hearths DROP NOT NULL,
    ADD COLUMN sites JSONB;
