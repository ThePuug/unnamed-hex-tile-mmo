-- A den as it stands on its site, the site's tile its key: what dens
-- there by its number, its level and its pack's size, the tile it stands
-- on, whether its nest is broken, and when it came to be so, in
-- milliseconds of the Unix clock. Its nest and pack are not kept.
CREATE TABLE den (
    chunk BIGINT  NOT NULL,
    q     INTEGER NOT NULL,
    r     INTEGER NOT NULL,
    state JSONB   NOT NULL,
    PRIMARY KEY (chunk, q, r)
);
