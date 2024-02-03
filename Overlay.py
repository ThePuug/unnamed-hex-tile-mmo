import pyglet
from pyglet.window import key

from Config import *
from HxPx import Hx, Px

PADDING = 10

class Overlay(pyglet.event.EventDispatcher):
    def __init__(self, batch):
        self.batch = batch
        self.curr = 0
        self.border = pyglet.shapes.Rectangle(0,0,0,0, color=(225, 225, 225, 255), batch=batch)
        self.border.width = TILE_WIDTH*3 + 4*PADDING
        self.border.height = TILE_HEIGHT + 2*PADDING
        self.border.anchor_x = self.border.width / 2
        self.border.anchor_y = -TILE_HEIGHT
        self.border.visible = False
        self.guides = [pyglet.shapes.Polygon(*[[it.x,it.y] for it in Hx(0,0,0).vertices()],color=(150,150,255,150), batch=batch) for _ in range(3)]
        for it in self.guides: 
            it.anchor_x = -TILE_WIDTH/2
            it.anchor_y = -TILE_HEIGHT/4
            it.visible = False
        self.display = [None for _ in range(3)]

    def on_key_press(self,sym,mod):
        if(sym == key.ESCAPE): 
            self.reset()
        if(sym == key.LEFT):
            self.curr = self.curr-1 if self.curr > 0 else len(self.opts)-1
            self.update_options()
        if(sym == key.RIGHT):
            self.curr = self.curr+1 if self.curr < len(self.opts)-1 else 0
            self.update_options()
        if(sym == key.SPACE):
            self.dispatch_event("on_select", self.hx, self.opts[self.curr])
            self.reset()
        return pyglet.event.EVENT_HANDLED

    def on_open(self,hx,opts):
        self.hx = hx
        self.opts = opts
        px = hx.into_px()
        self.border.position = (px.x,px.y)
        self.border.visible = True

        self.curr = 0
        for i in range(len(self.display)):
            pos = Px(px.x+TILE_WIDTH*(i-1)+PADDING*(i-1),px.y+TILE_HEIGHT*1.5+PADDING,hx.z)
            self.guides[i].position = (pos.x,pos.y)
            self.guides[i].visible = True
        self.update_options()

    def update_options(self):
        if self.curr is None: return
        px = self.hx.into_px()

        pos = Px(px.x+TILE_WIDTH*(0-1)+PADDING*(0-1),px.y+TILE_HEIGHT*1.5+PADDING,self.hx.z)
        if(self.display[0] is not None): self.display[0].delete()
        self.display[0] = self.opts[self.curr-1 if self.curr > 0 else len(self.opts)-1].create(pos, self.batch)

        pos = Px(px.x+TILE_WIDTH*(1-1)+PADDING*(1-1),px.y+TILE_HEIGHT*1.5+PADDING,self.hx.z)
        if(self.display[1] is not None): self.display[1].delete()
        self.display[1] = self.opts[self.curr].create(pos, self.batch)

        pos = Px(px.x+TILE_WIDTH*(2-1)+PADDING*(2-1),px.y+TILE_HEIGHT*1.5+PADDING,self.hx.z)
        if(self.display[2] is not None): self.display[2].delete()
        self.display[2] = self.opts[self.curr+1 if self.curr < len(self.opts)-1 else 0].create(pos, self.batch)
    
    def reset(self):
        self.dispatch_event("on_close")
        self.border.visible = False
        for it in self.guides: it.visible = False
        for it in self.display: 
            it.delete()
            it = None

Overlay.register_event_type('on_close')
Overlay.register_event_type('on_select')