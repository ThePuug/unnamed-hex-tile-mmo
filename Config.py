import logging
from math import sqrt

LOGLEVEL = logging.DEBUG

ISO_SCALE = 3/4

DEPTH = 10
TILE_SIZE = 24

# PRECALCULATED CONSTANTS
TILE_RISE = ISO_SCALE*(TILE_SIZE*2/3)
TILE_WIDTH = sqrt(3)*TILE_SIZE
TILE_HEIGHT = ISO_SCALE*TILE_SIZE*2

ONE_SQRT3 = 1/sqrt(3)
SQRT3 = sqrt(3)

ORIENTATION_PNTY = [[SQRT3, SQRT3/2, 0, 3/2],
                    [SQRT3/3, -1/3, 0, 2/3],
                    0.5]
ORIENTATION_FLAT = [[3/2, 0, SQRT3/2, SQRT3],
                    [2/3, 0, -1/3, SQRT3/3],
                    0.0]

FLAG_SOLID = 1 << 0
