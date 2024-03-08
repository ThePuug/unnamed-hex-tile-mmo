from math import floor
import random
from noise.perlin import SimplexNoise

from HxPx import Hx

class Impl:
    def __init__(self, seed):
        sz = floor(pow(2,16) / 100)-1
        random.seed(seed)
        noise = SimplexNoise(permutation_table=random.sample(range(sz), sz))
        self.octaves =  1
        self.freq = 64.0 * self.octaves
        self.tiles = {}
        for q in range(-sz,sz):
            for r in range(-sz, sz):
                self.tiles[Hx(q,r,-1)] = int(noise.noise2(q/self.freq, r/self.freq) * 127.0 + 128.0)

    def at(self, hx): return self.tiles.get(hx,None)
