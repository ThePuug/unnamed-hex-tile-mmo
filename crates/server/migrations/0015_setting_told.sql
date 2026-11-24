-- A change to a setting is told on the `setting` channel, whoever makes
-- it, so a running server hears what the admin console changed.
CREATE FUNCTION setting_told() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('setting', '');
    RETURN NULL;
END $$;

CREATE TRIGGER told AFTER INSERT OR UPDATE OR DELETE ON setting FOR EACH STATEMENT EXECUTE FUNCTION setting_told();
