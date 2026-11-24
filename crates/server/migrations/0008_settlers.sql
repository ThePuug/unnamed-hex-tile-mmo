-- A construct is its builder's or its hub's. One a settler raised, and a
-- hearth whose tent a player claimed, names no player: it names the camp
-- it is of, by the tile of that camp's hearth.
ALTER TABLE construct
    ALTER COLUMN provider DROP NOT NULL,
    ALTER COLUMN name DROP NOT NULL,
    ADD COLUMN camp_q INTEGER,
    ADD COLUMN camp_r INTEGER;

-- A camp, by the tile of the hearth it is round: when its settler comes,
-- in milliseconds of the Unix clock, and whether it has; the tiles its
-- tents and its wagon stand on; whether its spare tent is claimed and its
-- fortification paid for; and the materials it holds, as a pile's are
-- kept. Its settler is not kept.
CREATE TABLE camp (
    chunk BIGINT  NOT NULL,
    q     INTEGER NOT NULL,
    r     INTEGER NOT NULL,
    state JSONB   NOT NULL,
    PRIMARY KEY (chunk, q, r)
);
