from math import sqrt

TILE_SIZE = 48
ISO_SCALE = 1
RISE = ISO_SCALE*(2*TILE_SIZE/3)-2

HXM_R = 1
HXM_S = 3*HXM_R + 2
HXM_A = 3*pow(HXM_R,2)+3*HXM_R+1

# PRECALCULATED CONSTANTS
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
