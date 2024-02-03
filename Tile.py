import collision

from HxPx import Hx, Px
from Config import *

class Tile:
    SIZE = TILE_SIZE

    def __init__(self, pos, sprite, flags):
        self.flags = flags
        self.sprite = sprite
        self.collider = collision.Poly(collision.Vector(0,0),[collision.Vector(it.x,it.y) for it in Px(0,0).vertices(self.SIZE)])
        self.pos = pos

    @property
    def pos(self): return self._pos

    @pos.setter
    def pos(self, v):
        self._pos = v
        if type(v) is Hx: 
            self._hx = v
            self._px = v.into_px()
        elif type(v) is Px: 
            self._hx = v.into_hx()
            self._px = v
        self.collider.pos = collision.Vector(self._px.x,self._px.y)
        if self.sprite is not None: self.sprite.position = self._px.into_screen()

    @property
    def hx(self): return self._hx

    @property
    def px(self): return self._px

    def delete(self):
        if self.sprite is not None: 
            self.sprite.delete()
            self.sprite = None

    def __getstate__(self):
        return {
            "flags": self.flags,
            "sprite": {"typ": self.sprite._typ, "idx": self.sprite._idx}
        }
