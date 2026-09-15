"""A broadleaf tree: a bare trunk with a root flare, limbs leaving it just
under the crown, and a crown of overlapping lumpy spheres, one at the
centre, a ring around it at scattered heights, one on top, each squashed
its own way and in one of two leaf greens so the lumps read apart. The
crown is taller than it is wide and most of the tree; the whole tree leans
a little."""

import math

from mathutils import Vector

from .. import mesh, noise

BARK = (0.19, 0.14, 0.09)
LEAF = (0.10, 0.28, 0.08)
LEAF_LIT = (0.27, 0.50, 0.14)

## Crown width across and height, world units; the tile is 1.73 between
## flats, so the crown fills it and grows upward instead.
WIDTH = (1.5, 1.64)
CROWN = (2.1, 2.5)
## Bare trunk under the crown; a player is about 2.
CLEAR = (1.3, 1.6)


def build(p):
    s = p.seed
    r = lambda k: noise.hash01(k, 0, s)
    width = WIDTH[0] + (WIDTH[1] - WIDTH[0]) * r(1)
    crown_h = CROWN[0] + (CROWN[1] - CROWN[0]) * r(2)
    clear = CLEAR[0] + (CLEAR[1] - CLEAR[0]) * r(3)
    bm = mesh.bmesh_new()
    # Crown lumps as (centre, radius) about the origin: one at the centre, a
    # ring of four or five around it at their own distances and heights,
    # one on top. Built here, then scaled to the width and height and
    # lifted onto the trunk.
    lumps = [(Vector((0, 0, 0)), 0.9)]
    ring = 4 + int(r(4) * 2)
    for i in range(ring):
        a = 2 * math.pi * (i + r(5) + 0.5 * noise.hash01(i, 1, s)) / ring
        d = 0.4 + 0.25 * noise.hash01(i, 2, s)
        z = -0.5 + 0.7 * noise.hash01(i, 3, s)
        lumps.append((Vector((d * math.cos(a), d * math.sin(a), z)), 0.55 + 0.25 * noise.hash01(i, 4, s)))
    a = 2 * math.pi * r(6)
    lumps.append((Vector((0.25 * math.cos(a), 0.25 * math.sin(a), 0.6)), 0.7))
    # Greens alternate around the ring so no two neighbours merge into one
    # mass; which green leads is the seed's.
    swap = int(r(7) < 0.5)
    for i, (c, radius) in enumerate(lumps):
        verts = mesh.sphere(bm, 2, radius, c, material_index=1 + (i + swap) % 2)
        squash = Vector(tuple(0.88 + 0.24 * noise.hash01(i, k, s) for k in (6, 7, 8)))
        for v in verts:
            v.co = c + (v.co - c) * squash
        mesh.displace(bm, lambda co: 0.10 * noise.fbm(co * 2.5, 2, s ^ 0x40), verts)
    # Vertex handles die when the mesh grows, so the crown is re-read here,
    # and it is everything built so far.
    crown = list(bm.verts)
    lo = Vector(tuple(min(v.co[i] for v in crown) for i in range(3)))
    hi = Vector(tuple(max(v.co[i] for v in crown) for i in range(3)))
    k = Vector((width / max(hi.x - lo.x, hi.y - lo.y),) * 2 + (crown_h / (hi.z - lo.z),))
    lift = Vector((0, 0, clear - k.z * lo.z))
    for v in crown:
        v.co = v.co * k + lift
    # Trunk up into the centre lump, over a root flare whose top sits just
    # inside the trunk so no two faces coincide.
    mesh.cone(bm, 6, 0.18, 0.08, lift.z + 0.3 * k.z, 0.0, material_index=0)
    mesh.cone(bm, 6, 0.28, 0.16, 0.35, 0.0, material_index=0)
    # A limb to each ring lump from a point on the trunk under the crown,
    # so the branching shows between the trunk and the crown's underside.
    for i, (c, radius) in enumerate(lumps[1:-1]):
        start = (0, 0, clear - 0.25 - 0.3 * noise.hash01(i, 5, s))
        mesh.limb(bm, 4, 0.07, 0.03, start, c * k + lift, material_index=0)
    mesh.shear(bm, 0.04 * (r(8) - 0.5), 0.04 * (r(9) - 0.5))
    mesh.flat(bm)
    bark = mesh.material("deciduous-bark", BARK, roughness=1.0)
    leaf = mesh.material("deciduous-leaf", LEAF, roughness=1.0)
    lit = mesh.material("deciduous-leaf-lit", LEAF_LIT, roughness=1.0)
    return [mesh.object("deciduous-tree", bm, [bark, leaf, lit])]
