"""A pine: a tapered trunk under a stack of short, heavily overlapping cone
tiers whose rims are jittered, drooped, and turned to different phases so
no tier is a clean cone and no two line up, in two needle greens that
alternate so the tiers read apart. The whole tree leans a little."""

import math

from mathutils import Matrix

from .. import mesh, noise

TRUNK = (0.16, 0.10, 0.05)
NEEDLE = (0.035, 0.11, 0.055)
NEEDLE_LIT = (0.13, 0.27, 0.10)

## Height in world units; a player is about 2.
HEIGHT = (4.4, 5.8)
SEGMENTS = 7


def build(p):
    s = p.seed
    r = lambda k: noise.hash01(k, 0, s)
    height = HEIGHT[0] + (HEIGHT[1] - HEIGHT[0]) * r(1)
    tiers = 5 + int(r(2) * 2)
    bm = mesh.bmesh_new()
    # Trunk: visible below the first tier, and it carries on up inside the
    # foliage so gaps between tiers never show daylight.
    crown = 0.26 * height
    mesh.cone(bm, 6, 0.17, 0.05, height * 0.85, 0.0, material_index=0)
    # Tiers: each a cone starting a little inside the one below, wider at
    # the bottom and drawn in toward the tip, tall enough that each hides
    # the point of the one under it. Every rim vertex gets its own push out
    # and droop, and every tier its own turn.
    for i in range(tiers):
        t = i / (tiers - 1)
        base = crown + (height - crown) * t * 0.74
        radius = 0.62 * (1.0 - t) ** 0.85 + 0.10
        depth = (height - crown) * (0.40 - 0.12 * t)
        verts = mesh.cone(bm, SEGMENTS, radius, 0.0, depth, base, material_index=1 + i % 2)
        rim = [v for v in verts if abs(v.co.z - base) < 1e-4]
        for j, v in enumerate(rim):
            out = 0.82 + 0.36 * noise.hash01(j, i, s ^ 0x30)
            v.co.x *= out
            v.co.y *= out
            v.co.z -= 0.05 + 0.14 * noise.hash01(j, i, s ^ 0x31)
        turn = Matrix.Rotation(2 * math.pi * noise.hash01(i, 0, s ^ 0x32), 3, "Z")
        for v in verts:
            v.co = turn @ v.co
    # A lean: the top slides sideways by a small fraction of the height.
    mesh.shear(bm, 0.05 * (r(3) - 0.5), 0.05 * (r(4) - 0.5))
    mesh.flat(bm)
    trunk = mesh.material("pine-trunk", TRUNK, roughness=1.0)
    needle = mesh.material("pine-needle", NEEDLE, roughness=1.0)
    lit = mesh.material("pine-needle-lit", NEEDLE_LIT, roughness=1.0)
    return [mesh.object("pine-tree", bm, [trunk, needle, lit])]
