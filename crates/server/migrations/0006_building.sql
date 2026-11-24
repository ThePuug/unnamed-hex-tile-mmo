-- A cover's seven slots take eight bits each where they took four, so a
-- slot names a construct in its material, and the boulders' rock follows
-- them from bit 56. Each kept cover is laid out anew.
ALTER TABLE tile_cover ALTER COLUMN cover TYPE BIGINT USING (
      (cover::BIGINT        & 15)
    | (cover::BIGINT >> 4   & 15) << 8
    | (cover::BIGINT >> 8   & 15) << 16
    | (cover::BIGINT >> 12  & 15) << 24
    | (cover::BIGINT >> 16  & 15) << 32
    | (cover::BIGINT >> 20  & 15) << 40
    | (cover::BIGINT >> 24  & 15) << 48
    | (cover::BIGINT >> 28  & 3)  << 56
);

-- Whose each construct is, by its tile: its builder as a provider's name
-- for a player. What it is, the tile's cover says.
CREATE TABLE construct (
    chunk    BIGINT  NOT NULL,
    q        INTEGER NOT NULL,
    r        INTEGER NOT NULL,
    provider TEXT    NOT NULL,
    name     TEXT    NOT NULL,
    PRIMARY KEY (chunk, q, r)
);

-- A hub: the hearths that are one, as a JSON list of `[q, r]`, by a
-- number of no meaning.
CREATE TABLE hub (
    id      BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    hearths JSONB  NOT NULL
);

-- When a character was last in the world: a hub is as large as the
-- players at home in it who have played of late.
ALTER TABLE player_character ADD COLUMN seen TIMESTAMPTZ NOT NULL DEFAULT now();
