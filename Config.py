import logging
from math import sqrt

LOGLEVEL = logging.DEBUG

SERVER = "localhost"
SERVER_PORT = 42424

ISO_SCALE = 3/4

DEPTH = 10
TILE_SIZE = 24

LOAD_BUF_SIZE = pow(2,16)

# PRECALCULATED CONSTANTS
ONE_SQRT3 = 1/sqrt(3)
SQRT3 = sqrt(3)

TILE_SIZE_H = TILE_SIZE
TILE_SIZE_W = round(TILE_SIZE * SQRT3) / SQRT3
TILE_RISE = ISO_SCALE*(TILE_SIZE_H*2/3)
TILE_WIDTH = SQRT3*TILE_SIZE_W
TILE_HEIGHT = ISO_SCALE*TILE_SIZE_H*2

ORIENTATION_PNTY = [[SQRT3, SQRT3/2, 0, 3/2],
                    [SQRT3/3, -1/3, 0, 2/3],
                    0.5]
ORIENTATION_FLAT = [[3/2, 0, SQRT3/2, SQRT3],
                    [2/3, 0, -1/3, SQRT3/3],
                    0.0]

FLAG_NONE  = 0
FLAG_SOLID = 1 << 0

# UTILITIES
New = lambda **kwargs: type("Object", (), kwargs)
