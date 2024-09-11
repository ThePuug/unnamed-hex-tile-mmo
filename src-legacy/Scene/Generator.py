import math
import random
from noise.perlin import SimplexNoise

ELEVATION = 0
VEGETATION = 1

class Impl:
    def __init__(self, seed):
        self.tiles = {}
        random.seed(seed)

        period = pow(2,10)
        self._elevation = SimplexNoise(permutation_table=random.sample(range(period), period))
        self._vegetation = SimplexNoise(permutation_table=random.sample(range(period), period))

    def elevation(self, hx): return math.floor((self._elevation.noise2(hx.q/1024.0, hx.r/1024.0)/2.0+0.5)*100)
    def vegetation(self, hx): return math.floor((self._vegetation.noise2(hx.q/16.0, hx.r/16.0)/2.0+0.5)*100)
