-- The world's calendar as an admin left it: how far it stands from the
-- wall clock, in milliseconds, and the moment it is held at, if it is.
-- Every time kept is a moment of this calendar, so a server that starts
-- reads them on the calendar they were kept on. One row, or none where
-- the calendar has never been moved.
CREATE TABLE calendar (
    one   BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (one),
    shift BIGINT  NOT NULL,
    held  BIGINT
);

-- When a character was last in the world is a moment of the calendar, in
-- its milliseconds, where it was the database's own clock.
ALTER TABLE player_character
    ALTER COLUMN seen DROP DEFAULT,
    ALTER COLUMN seen TYPE BIGINT USING (EXTRACT(EPOCH FROM seen) * 1000)::BIGINT,
    ALTER COLUMN seen SET DEFAULT 0;

-- A den and a camp kept their times on the Unix clock in milliseconds,
-- which the calendar runs a zone's hours ahead of: each reads those few
-- hours older than it is, and no row changes.
