from math import floor
from noise import snoise2

from HxPx import Hx

class Generator:
    def __init__(self):
        self.octaves =  2 # TODO magic number
        self.sz = 16 # floor(pow(2,16) / 100)-1
        self.freq = 32.0 * self.octaves # TODO magic number
        self.tiles = {}
        for q in range(-self.sz,self.sz):
            for r in range(-self.sz, self.sz):
                self.tiles[Hx(q,r,-1)] = int(snoise2(q/self.freq, r/self.freq, self.octaves) * 127.0 + 128.0)

    def at(self, hx): return self.tiles.get(hx,None)
