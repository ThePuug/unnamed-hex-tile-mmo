-- The players and their characters.

-- A player. `epoch` rises each time a server claims the account, as a
-- character of it enters the world, and a write naming an older one is
-- refused: one server writes an account's characters at a time.
CREATE TABLE account (
    id     BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    epoch  BIGINT NOT NULL DEFAULT 0,
    holder TEXT   NOT NULL DEFAULT ''
);

-- A provider's name for a player. Many name one account, so a player is the
-- same account from every platform.
CREATE TABLE identity (
    provider TEXT   NOT NULL,
    name     TEXT   NOT NULL,
    account  BIGINT NOT NULL REFERENCES account,
    PRIMARY KEY (provider, name)
);

-- A character of an account, as it last left the world or was last written
-- in it: JSON `null` until it first is.
CREATE TABLE player_character (
    id      BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    account BIGINT NOT NULL REFERENCES account,
    state   JSONB  NOT NULL
);

CREATE INDEX player_character_account ON player_character (account);
