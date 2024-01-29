import pyglet
from pyglet.window import key
from Assets import LayerSet

from HxPx import Hx, Px
from Tile import TileSet

PADDING = 10

class Overlay(pyglet.event.EventDispatcher):
    def __init__(self, batch, groups):
        self.curr = 0

        self.border = pyglet.shapes.Rectangle(0,0,0,0, color=(225, 225, 225, 255), batch=batch, group=groups[0])
        hx = Hx(0,0,0)
        self.border.width = hx.width*3 + 4*PADDING
        self.border.height = hx.height + 2*PADDING
        self.border.anchor_x = self.border.width / 2
        self.border.anchor_y = -5/4*hx.height
        self.border.visible = False
        self.display = [TileSet(Px(0,0,0),LayerSet([],(0,0)),batch,groups) for _ in range(3)]

    def on_key_press(self,sym,mod):
        if(sym == key.ESCAPE): 
            self.dispatch_event("on_close")
            self.border.visible = False
            for it in self.display: it.visible = False
        if(sym == key.LEFT):
            self.curr = self.curr-1 if self.curr > 0 else len(self.opts)-1
            self.update_options()
        if(sym == key.RIGHT):
            self.curr = self.curr+1 if self.curr < len(self.opts)-1 else 0
            self.update_options()
        if(sym == key.SPACE):
            self.dispatch_event("on_close")
            self.dispatch_event("on_select", self.hx, self.opts[self.curr])
            self.border.visible = False
            for it in self.display: it.visible = False
        return pyglet.event.EVENT_HANDLED

    def on_open(self,hx,opts):
        self.hx = hx
        self.opts = opts
        px = hx.into_px()
        self.border.position = (px.x,px.y)
        self.border.visible = True

        self.curr = 0
        for i,it in enumerate(self.display):
            it.layers = LayerSet([],(0,0))
            it.pos = Px(px.x+hx.width*(i-1)+PADDING*(i-1),px.y+hx.height*1.5+PADDING,0)
            it.visible = True
        self.update_options()

    def update_options(self):
        if self.curr is None: return
        self.display[0].layers = self.opts[self.curr-1 if self.curr > 0 else len(self.opts)-1]
        self.display[1].layers = self.opts[self.curr]
        self.display[2].layers = self.opts[self.curr+1 if self.curr < len(self.opts)-1 else 0]

Overlay.register_event_type('on_close')
Overlay.register_event_type('on_select')