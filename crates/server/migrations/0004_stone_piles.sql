-- A pile's place in its tile's cover says which stone it is, where it said
-- only that it was stone, drawn as the rock it lay on: 10 is sandstone,
-- 11 limestone and 12 basement. Each kept pile of limestone (material 4)
-- or basement (5) takes its own.
UPDATE tile_cover t
SET cover = (t.cover & ~(15 << (4 * p.slot))) | ((7 + (p.stacks -> 0 ->> 'material')::INT) << (4 * p.slot))
FROM pile p
WHERE p.chunk = t.chunk AND p.q = t.q AND p.r = t.r
  AND (p.stacks -> 0 ->> 'material')::INT IN (4, 5)
  AND (t.cover >> (4 * p.slot)) & 15 = 10;
