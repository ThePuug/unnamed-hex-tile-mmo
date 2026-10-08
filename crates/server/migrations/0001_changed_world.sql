-- The ground as players left it. Every row is keyed by the chunk holding
-- it, packed as `(n << 32) | (m as u32)`: the one column a sharded store
-- distributes by, and the unit a server claims.

-- Who writes each chunk. `epoch` rises with every claim, and a write naming
-- an older one is refused.
CREATE TABLE chunk_owner (
    chunk  BIGINT PRIMARY KEY,
    epoch  BIGINT NOT NULL,
    holder TEXT   NOT NULL
);

-- A tile's cover as players left it, as `common::Cover`'s bits.
CREATE TABLE tile_cover (
    chunk BIGINT  NOT NULL,
    q     INTEGER NOT NULL,
    r     INTEGER NOT NULL,
    cover INTEGER NOT NULL,
    PRIMARY KEY (chunk, q, r)
);

-- A summary reads the changes within a box of tiles, across chunks.
CREATE INDEX tile_cover_qr ON tile_cover (q, r);

-- What a pile holds, as a JSON list of `common::Stack`.
CREATE TABLE pile (
    chunk  BIGINT   NOT NULL,
    q      INTEGER  NOT NULL,
    r      INTEGER  NOT NULL,
    slot   SMALLINT NOT NULL,
    stacks JSONB    NOT NULL,
    PRIMARY KEY (chunk, q, r, slot)
);
