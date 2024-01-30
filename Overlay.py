import pyglet
from pyglet.window import key
from Assets import LayerSet

from HxPx import Hx, Px, inv_hexmod
from Tile import TileSet

PADDING = 10

class Overlay(pyglet.event.EventDispatcher):
    def __init__(self, batch, groups):
        self.curr = 0
        self.with_hxm = False
        self.hxm = 0

        self.border = pyglet.shapes.Rectangle(0,0,0,0, color=(225, 225, 225, 255), batch=batch, group=groups[0])
        hx = Hx(0,0,0)

        self.border.width = hx.width*3 + 4*PADDING
        self.border.height = hx.height + 2*PADDING
        self.border.anchor_x = self.border.width / 2
        self.border.anchor_y = -hx.height
        self.border.visible = False

        self.guides = [pyglet.shapes.Polygon(*[[it.x,it.y] for it in hx.vertices],color=(150,150,255,150), batch=batch, group=groups[0]) for _ in range(3)]
        for it in self.guides: 
            it.anchor_x = -hx.width/2
            it.anchor_y = -hx.height/4
            it.visible = False

        self.display = [TileSet(Px(0,0,0),LayerSet([],(0,0)),batch,groups) for _ in range(3)]

    def on_key_press(self,sym,mod):
        if(sym == key.ESCAPE): 
            self.reset()
        if(sym == key.LEFT):
            self.curr = self.curr-1 if self.curr > 0 else len(self.opts)-1
            self.update_options()
        if(sym == key.RIGHT):
            self.curr = self.curr+1 if self.curr < len(self.opts)-1 else 0
            self.update_options()
        if(sym == key.A):
            self.hxm = self.hxm-1 if self.hxm > 0 else 6
            self.update_hxm()
        if(sym == key.D):
            self.hxm = self.hxm+1 if self.hxm < 6 else 0
            self.update_hxm()
        if(sym == key.SPACE):
            self.dispatch_event("on_select", self.hx, None if self.with_hxm == False else self.hxm, self.opts[self.curr])
            self.reset()
        return pyglet.event.EVENT_HANDLED

    def on_open(self,hx,with_hxm,opts):
        self.hx = hx
        self.with_hxm = with_hxm
        self.opts = opts
        px = hx.into_px()
        self.border.position = (px.x,px.y)
        self.border.visible = True

        self.curr = 0
        for i,it in enumerate(self.display):
            it.layers = LayerSet([],(0,0))
            it.pos = Px(px.x+hx.width*(i-1)+PADDING*(i-1),px.y+hx.height*1.5+PADDING,0)
            it.visible = True
            self.guides[i].position = (px.x+hx.width*(i-1)+PADDING*(i-1),px.y+hx.height*1.5+PADDING)
            self.guides[i].visible = True
        self.update_options()

    def update_options(self):
        if self.curr is None: return
        self.display[0].layers = self.opts[self.curr-1 if self.curr > 0 else len(self.opts)-1]
        self.display[1].layers = self.opts[self.curr]
        self.display[2].layers = self.opts[self.curr+1 if self.curr < len(self.opts)-1 else 0]
    
    def update_hxm(self):
        if not self.with_hxm: return
        for i,it in enumerate(self.display):
            offset = inv_hexmod(self.hxm)
            it.pos = Px(self.guides[i].x+offset[0],self.guides[i].y+offset[1],0)

    def reset(self):
        self.dispatch_event("on_close")
        self.border.visible = False
        for it in self.guides: it.visible = False
        for it in self.display: it.visible = False
        self.hxm = 0

Overlay.register_event_type('on_close')
Overlay.register_event_type('on_select')