-- 0006 laid each kept cover out anew and lost every slot of it: its
-- shifts and ors, of one precedence in SQL, ran left to right, so only
-- the rock came through. What stood in a slot is gone, but a pile is
-- kept apart from its cover, by its tile and slot, with what it holds. So
-- each kept pile whose slot reads empty is laid in its cover again, as
-- the pile of its first stack's material: softwood 8, hardwood 9,
-- sandstone 10, limestone 11, basement 12, hide 15.
UPDATE tile_cover t
SET cover = t.cover | lost.bits
FROM (
    SELECT c.chunk, c.q, c.r, bit_or(m.content << (8 * p.slot)) AS bits
    FROM pile p
    JOIN tile_cover c ON c.chunk = p.chunk AND c.q = p.q AND c.r = p.r
    JOIN (VALUES (1, 8::BIGINT), (2, 9), (3, 10), (4, 11), (5, 12), (6, 15)) AS m(material, content)
        ON m.material = (p.stacks -> 0 ->> 'material')::INT
    WHERE ((c.cover >> (8 * p.slot)) & 255) = 0
    GROUP BY c.chunk, c.q, c.r
) lost
WHERE t.chunk = lost.chunk AND t.q = lost.q AND t.r = lost.r;
