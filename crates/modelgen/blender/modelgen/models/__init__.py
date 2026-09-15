"""Every model, by name. Adding one: a module with
`build(Params) -> list[bpy.types.Object]`, an entry in `MODELS`, a row in
the README."""

from dataclasses import dataclass
from typing import Callable

from . import boulder, deciduous_tree, pine_tree


@dataclass(frozen=True)
class Params:
    seed: int


@dataclass(frozen=True)
class Model:
    name: str
    ## What the model must read as. The comparison critic judges against it.
    brief: str
    ## Extent it must fit, world units, x by y by z. A prop on a tile top
    ## fits between the hex's flat edges: 1.73 across.
    box: tuple
    ## Triangle budget.
    tris: int
    build: Callable[[Params], list]


MODELS = [
    Model(
        name="boulder",
        brief="A single boulder for hex tile tops, seen from a few metres up: "
              "one rounded lump of the terrain's cliff grey, wider than tall, "
              "low-poly with flat facets, settled into the ground rather than "
              "resting on it, a little lichen on its upper faces.",
        box=(1.6, 1.6, 1.0),
        tris=700,
        build=boulder.build,
    ),
    Model(
        name="pine-tree",
        brief="A pine tree for hex tiles, seen from a few metres up: one straight "
              "trunk under a stack of ragged conical tiers of dark needles, "
              "narrowing to a point, two to three times a person's height, low-poly with flat "
              "facets, standing on the ground.",
        box=(1.7, 1.7, 6.0),
        tris=600,
        build=pine_tree.build,
    ),
    Model(
        name="deciduous-tree",
        brief="A broadleaf tree for hex tiles, seen from a few metres up: one bare "
              "trunk with a few limbs leaving it under a full, rounded, lumpy crown "
              "of leaf green about as wide as the tile, half again to twice a "
              "person's height, low-poly with flat facets, standing on the ground.",
        box=(1.7, 1.7, 4.5),
        tris=700,
        build=deciduous_tree.build,
    ),
]


def find(name):
    for m in MODELS:
        if m.name == name:
            return m
    raise KeyError(f"no model `{name}`")
