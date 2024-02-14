import quickle

import Tile
from Event import REGISTRY

ENCODER = quickle.Encoder(registry=REGISTRY+[Tile.State])
DECODER = quickle.Decoder(registry=REGISTRY+[Tile.State])
