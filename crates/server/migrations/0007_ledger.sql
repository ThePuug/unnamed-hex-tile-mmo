-- Every economy interaction, appended in the transaction that writes its
-- holders and never changed after: what moved, how many, from which
-- holder, to which, and what caused it. `item` is a kind of thing by its
-- number, `{"material": n}` or `{"piece": n, "seed": s}`. A holder is
-- `{"character": id}`, `{"pile": [q, r, slot]}` or `"world"`, which
-- holds whatever is not in play: the sum of what left it less what
-- returned is everything in play. `cause` is the cause's number, and
-- `npc` the archetype's where an NPC's death was the cause.
CREATE TABLE ledger (
    id     BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    item   JSONB       NOT NULL,
    count  BIGINT      NOT NULL CHECK (count > 0),
    source JSONB       NOT NULL,
    dest   JSONB       NOT NULL,
    cause  INTEGER     NOT NULL,
    npc    INTEGER
);

-- A holder's entries are read to audit what it holds.
CREATE INDEX ledger_source ON ledger USING HASH (source);
CREATE INDEX ledger_dest ON ledger USING HASH (dest);
