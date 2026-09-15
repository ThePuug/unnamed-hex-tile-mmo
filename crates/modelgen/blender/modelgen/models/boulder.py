"""A boulder: one icosphere squashed and leaned, lumped by noise, cleaved
flat on the top and two sides, cut off at its widest where it meets the
ground. Stone is the cliff grey of `terrain.wgsl`; lichen is the
mountain-scree lichen."""

import math

from .. import mesh, noise

## terrain.wgsl cliff grey, linear RGB, as texgen's cliff-stone uses it.
STONE = (0.35, 0.32, 0.28)
LICHEN = (0.46, 0.50, 0.20)


def build(p):
    s = p.seed
    r = lambda k: noise.hash01(k, 0, s)
    bm = mesh.icosphere(subdivisions=3, radius=1.0)
    # Wider than tall, longer one way than the other, and leaning, so no
    # two views share an outline.
    mesh.scale(bm, 0.72, 0.58, 0.66)
    mesh.shear(bm, 0.36 * (r(1) - 0.5), 0.36 * (r(2) - 0.5))
    # Three lump scales: a broad swelling that shifts the mass to one side,
    # medium swellings, and small chips for the facets to catch. The broad
    # one must span about a lattice cell across the rock or it only
    # translates it.
    mesh.displace(bm, lambda co: 0.14 * noise.perlin(co * 2.0, s ^ 0x10)
                                  + 0.07 * noise.fbm(co * 3.5, 2, s ^ 0x11)
                                  + 0.025 * noise.fbm(co * 7.0, 2, s ^ 0x12))
    # Cleaved faces: a tilted top and two sides, the second at least a
    # quarter turn from the first so they never double up into one sharp
    # corner, so the rock reads as fractured block rather than a worn dome.
    mesh.facet(bm, (0.5 * (r(3) - 0.5), 0.5 * (r(4) - 0.5), 1.0), 0.50 + 0.08 * r(5))
    side = 2 * math.pi * r(6)
    for a, tilt, k in ((side, 0.1, 8), (side + math.pi * (0.5 + r(7)), 0.5, 9)):
        mesh.facet(bm, (math.cos(a), math.sin(a), tilt), 0.44 + 0.1 * r(k))
    # Cut at the widest section so the base is the widest part and nothing
    # curls under.
    mesh.bury(bm, depth=0.62)
    mesh.flat(bm)
    # Lichen: a few small patches on faces that look up, more of them
    # toward the crown. The mask is fine enough that one patch never covers
    # a whole side.
    bm.normal_update()
    top = max(v.co.z for v in bm.verts)
    for f in bm.faces:
        c = f.calc_center_median()
        crown = noise.smoothstep(0.2, 1.0, c.z / top)
        if f.normal.z > 0.65 and noise.fbm(c * 4.5, 2, s ^ 0x21) + 0.25 * crown > 0.40:
            f.material_index = 1
    stone = mesh.material("boulder-stone", STONE, roughness=0.9)
    lichen = mesh.material("boulder-lichen", LICHEN, roughness=1.0)
    return [mesh.object("boulder", bm, [stone, lichen])]
