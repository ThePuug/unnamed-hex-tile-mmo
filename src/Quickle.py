import quickle

import Tile
import Actor
from Event import REGISTRY

ENCODER = quickle.Encoder(registry=REGISTRY+[Tile.State, Actor.State])
DECODER = quickle.Decoder(registry=REGISTRY+[Tile.State, Actor.State])
