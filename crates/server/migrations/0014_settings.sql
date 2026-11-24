-- The numbers an admin changed from what the server was built with, each
-- by its setting's number (`common_bevy::settings::Setting::id`). One a
-- build has no setting for stays as it is. Not journaled: a rollback
-- takes the world back to an earlier moment, and how the server is tuned
-- is not the world's.
CREATE TABLE setting (
    id    INT PRIMARY KEY,
    value INT NOT NULL
);
