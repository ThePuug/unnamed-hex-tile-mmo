import collision
import quickle

from HxPx import Hx, Px
from Config import *

class State(quickle.Struct):
    flags: int = 0
    sprite__typ: str
    sprite__idx: int

class Tile:
    SIZE = TILE_SIZE

    def __init__(self, px, sprite, flags):
        self.flags = flags
        self.sprite = sprite
        self.collider = collision.Poly(collision.Vector(0,0),[collision.Vector(it.x,it.y) for it in Px(0,0,0).vertices(self.SIZE)])
        self.px = px

    @property
    def hx(self): return self._hx

    @hx.setter
    def hx(self,v): 
        self._hx = v
        self._px = v.into_px()

    @property
    def px(self): return self._px

    @px.setter
    def px(self, v):
        self._px = v
        self._hx = v.into_hx()

    def delete(self):
        if self.sprite is not None: 
            self.sprite.delete()
            self.sprite = None

    @property
    def state(self):
        return State(flags          = self.flags, 
                     sprite__typ    = self.sprite._typ, 
                     sprite__idx    = self.sprite._idx)