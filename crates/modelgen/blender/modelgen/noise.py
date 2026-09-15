"""Seeded noise over world coordinates. Every function is a pure function
of its inputs, so a model built twice from one seed is the same mesh."""

import math

from mathutils import Vector
from mathutils import noise as mn


def hash01(i, k, seed):
    """Hash of an index and a lane to [0, 1)."""
    h = (i * 0x9E3779B1 ^ k * 0x85EBCA77 ^ seed * 0xC2B2AE3D) & 0xFFFFFFFF
    h ^= h >> 15
    h = (h * 0x2C1B3C6D) & 0xFFFFFFFF
    h ^= h >> 12
    return h / 0x100000000


def offset(seed):
    """Where a seed samples the noise field: far enough apart that two seeds
    never share a feature."""
    return Vector((hash01(1, 0, seed), hash01(2, 0, seed), hash01(3, 0, seed))) * 1000.0


def perlin(co, seed):
    """Gradient noise at `co`, in about [-1, 1]."""
    return mn.noise(co + offset(seed))


def fbm(co, octaves, seed, gain=0.5):
    """Fractal sum of `octaves` gradient layers, each twice as fine and
    `gain` as strong. Normalised to about [-1, 1]."""
    total, amp, norm = 0.0, 1.0, 0.0
    p = co + offset(seed)
    for _ in range(octaves):
        total += amp * mn.noise(p)
        norm += amp
        p = p * 2.0 + Vector((17.3, 31.7, 5.1))
        amp *= gain
    return total / norm


def smoothstep(e0, e1, x):
    t = min(max((x - e0) / (e1 - e0), 0.0), 1.0)
    return t * t * (3.0 - 2.0 * t)
