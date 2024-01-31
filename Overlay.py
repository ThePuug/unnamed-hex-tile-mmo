import pyglet
from pyglet.window import key

from Config import *
from Assets import LayerSet
from HxPx import Hx, Px
from Tile import HexSet

PADDING = 10

class Overlay(pyglet.event.EventDispatcher):
    def __init__(self, batch, groups):
        self._hxm = None
        self.curr = 0
        self.border = pyglet.shapes.Rectangle(0,0,0,0, color=(225, 225, 225, 255), batch=batch, group=groups[0])
        self.border.width = TILE_WIDTH*3 + 4*PADDING
        self.border.height = TILE_HEIGHT + 2*PADDING
        self.border.anchor_x = self.border.width / 2
        self.border.anchor_y = -TILE_HEIGHT
        self.border.visible = False
        self.guides = [pyglet.shapes.Polygon(*[[it.x,it.y] for it in Hx(0,0,0).vertices()],color=(150,150,255,150), batch=batch, group=groups[0]) for _ in range(3)]
        for it in self.guides: 
            it.anchor_x = -TILE_WIDTH/2
            it.anchor_y = -TILE_HEIGHT/4
            it.visible = False
        self.display = [HexSet(Px(0,0,0),LayerSet([],(0,0)),batch,groups) for _ in range(3)]

    def on_key_press(self,sym,mod):
        if(sym == key.ESCAPE): 
            self.reset()
        if(sym == key.LEFT):
            self.curr = self.curr-1 if self.curr > 0 else len(self.opts)-1
            self.update_options()
        if(sym == key.RIGHT):
            self.curr = self.curr+1 if self.curr < len(self.opts)-1 else 0
            self.update_options()
        if(self.hxm is not None and sym == key.A):
            self.hxm = self.hxm-1 if self.hxm > 0 else 6
        if(self.hxm is not None and sym == key.D):
            self.hxm = self.hxm+1 if self.hxm < 6 else 0
        if(sym == key.SPACE):
            self.dispatch_event("on_select", self.hx, self.hxm, self.opts[self.curr])
            self.reset()
        return pyglet.event.EVENT_HANDLED

    def on_open(self,hx,hxm,opts):
        self.hx = hx
        self.hxm = hxm
        self.opts = opts
        px = hx.into_px()
        self.border.position = (px.x,px.y)
        self.border.visible = True

        self.curr = 0
        for i,it in enumerate(self.display):
            it.layers = LayerSet([],(0,0))
            it.hxm = hxm
            it.pos = Px(px.x+TILE_WIDTH*(i-1)+PADDING*(i-1),px.y+TILE_HEIGHT*1.5+PADDING,0)
            it.visible = True
            self.guides[i].position = (px.x+TILE_WIDTH*(i-1)+PADDING*(i-1),px.y+TILE_HEIGHT*1.5+PADDING)
            self.guides[i].visible = True
        self.update_options()

    def update_options(self):
        if self.curr is None: return
        self.display[0].layers = self.opts[self.curr-1 if self.curr > 0 else len(self.opts)-1]
        self.display[1].layers = self.opts[self.curr]
        self.display[2].layers = self.opts[self.curr+1 if self.curr < len(self.opts)-1 else 0]
    
    @property
    def hxm(self): return self._hxm

    @hxm.setter
    def hxm(self, v):
        self._hxm = v
        for it in self.display: it.hxm = v

    def reset(self):
        self.dispatch_event("on_close")
        self.border.visible = False
        for it in self.guides: it.visible = False
        for it in self.display: it.visible = False
        self.hxm = 0

Overlay.register_event_type('on_close')
Overlay.register_event_type('on_select')