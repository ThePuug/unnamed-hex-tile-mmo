-- Everything kept is known by the number given it once, where it was
-- known by the name the build of the day gave it: a piece by its asset's
-- name, a material and a skill by its name in the code. The names below
-- are those of that day, each with the number it was then given.

CREATE TEMP TABLE piece_number (name TEXT PRIMARY KEY, id INT NOT NULL);
INSERT INTO piece_number VALUES
    ('ranger-hood', 1), ('ranger-vest', 2), ('ranger-gloves', 3), ('ranger-girdle', 4),
    ('ranger-pants', 5), ('ranger-boots', 6), ('acolyte-cap', 7), ('acolyte-robe', 8),
    ('acolyte-wraps', 9), ('acolyte-sash', 10), ('acolyte-hose', 11), ('acolyte-shoes', 12),
    ('corsair-hat', 13), ('corsair-jacket', 14), ('corsair-gauntlets', 15), ('corsair-sash', 16),
    ('corsair-breeches', 17), ('corsair-boots', 18), ('hauberk-helm', 19), ('hauberk-shirt', 20),
    ('hauberk-mittens', 21), ('hauberk-belt', 22), ('hauberk-hose', 23), ('hauberk-shoes', 24),
    ('lamellar-helm', 25), ('lamellar-coat', 26), ('lamellar-bracers', 27), ('lamellar-sash', 28),
    ('lamellar-trousers', 29), ('lamellar-boots', 30), ('brigandine-sallet', 31),
    ('brigandine-jack', 32), ('brigandine-bracers', 33), ('brigandine-belt', 34),
    ('brigandine-hose', 35), ('brigandine-boots', 36), ('sentinel-helm', 37), ('sentinel-cuirass',
    38), ('sentinel-gauntlets', 39), ('sentinel-leggings', 40), ('sentinel-sabatons', 41),
    ('crusader-helm', 42), ('crusader-cuirass', 43), ('crusader-gauntlets', 44),
    ('crusader-leggings', 45), ('crusader-sabatons', 46), ('gothic-helm', 47), ('gothic-cuirass',
    48), ('gothic-gauntlets', 49), ('gothic-leggings', 50), ('gothic-sabatons', 51),
    ('arming-sword', 52), ('falchion', 53), ('sabre', 54), ('bearded-axe', 55), ('broad-axe', 56),
    ('double-axe', 57), ('flanged-mace', 58), ('morning-star', 59), ('war-hammer', 60),
    ('sword-breaker', 61);

CREATE TEMP TABLE material_number (name TEXT PRIMARY KEY, id INT NOT NULL);
INSERT INTO material_number VALUES
    ('Softwood', 1), ('Hardwood', 2), ('Sandstone', 3), ('Limestone', 4), ('Basement', 5);

CREATE TEMP TABLE skill_number (name TEXT PRIMARY KEY, id INT NOT NULL);
INSERT INTO skill_number VALUES
    ('AutoAttack', 1), ('Frenzy', 2), ('Feint', 3), ('Overpower', 4), ('Punish', 5), ('Parry', 6),
    ('Counter', 7), ('Leap', 8), ('PerfectStride', 9), ('Clash', 10), ('Sidestep', 11), ('Yank',
    12), ('Backstab', 13), ('Pounce', 14), ('Kick', 15), ('Defy', 16), ('Bloodlust', 17), ('Brace',
    18), ('Surge', 19), ('Vigil', 20), ('Lurk', 21), ('Drag', 22);

CREATE FUNCTION pg_temp.items(named JSONB) RETURNS JSONB LANGUAGE sql AS $$
    SELECT COALESCE(jsonb_agg(jsonb_build_object('piece', n.id, 'seed', e -> 'seed') ORDER BY at), '[]')
    FROM jsonb_array_elements(named) WITH ORDINALITY AS t(e, at)
    JOIN piece_number n ON n.name = e ->> 'piece'
$$;

CREATE FUNCTION pg_temp.stacks(named JSONB) RETURNS JSONB LANGUAGE sql AS $$
    SELECT COALESCE(jsonb_agg(jsonb_build_object('material', n.id, 'count', e -> 'count') ORDER BY at), '[]')
    FROM jsonb_array_elements(named) WITH ORDINALITY AS t(e, at)
    JOIN material_number n ON n.name = e -> 'kind' ->> 'Material'
$$;

CREATE FUNCTION pg_temp.skills(named JSONB) RETURNS JSONB LANGUAGE sql AS $$
    SELECT COALESCE(jsonb_agg(n.id ORDER BY at), '[]')
    FROM jsonb_array_elements_text(named) WITH ORDINALITY AS t(e, at)
    JOIN skill_number n ON n.name = e
$$;

UPDATE player_character
SET state = state || jsonb_build_object(
    'owned', pg_temp.items(state -> 'owned'),
    'worn', pg_temp.items(state -> 'worn'),
    'stock', pg_temp.stacks(state -> 'stock'),
    'loadout', pg_temp.skills(state -> 'loadout'))
WHERE jsonb_typeof(state) = 'object';

UPDATE pile SET stacks = pg_temp.stacks(stacks);

DROP FUNCTION pg_temp.items(JSONB);
DROP FUNCTION pg_temp.stacks(JSONB);
DROP FUNCTION pg_temp.skills(JSONB);
DROP TABLE piece_number;
DROP TABLE material_number;
DROP TABLE skill_number;
