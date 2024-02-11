import pyglet
from pyglet.window import key

from Config import *
from HxPx import Px

PADDING = 10

class Border:
    def __init__(self, pos, size, asset_factory, batch):
        self.size = size
        self.sprites = [asset_factory.create_sprite("ui", 0, batch, pos),
                        asset_factory.create_sprite("ui", 1, batch, pos),
                        asset_factory.create_sprite("ui", 0, batch, pos)]
        for i,it in enumerate(self.sprites):
            it.scale_y = size.y / it.height
            it.visible = False
        self.sprites[1].scale_x = (size.x-self.sprites[0].width-self.sprites[2].width) / self.sprites[1].width

    @property
    def visible(self): return self.sprites[0].visible

    @visible.setter
    def visible(self, v):
        for it in self.sprites: it.visible = v

    @property
    def position(self): return self.sprites[1].position

    @position.setter
    def position(self, v):
        for i,it in enumerate(self.sprites): 
            it.x = v.x+(i-1)*(self.size.x-it.width)/2
            it.y = v.y

class Overlay(pyglet.event.EventDispatcher):
    def __init__(self, asset_factory, batch):
        self.asset_factory = asset_factory
        self.batch = batch
        self.curr = 0
        self.border = Border(Px(0,0,0), Px(TILE_WIDTH*3+4*PADDING, TILE_HEIGHT+2*PADDING), asset_factory, batch)
        self.guides = [asset_factory.create_sprite("terrain", 5, batch, Px(0,0,0)) for _ in range(3)]
        for it in self.guides: it.visible = False
        self.display = [None for _ in range(3)]

    def on_key_press(self,sym,mod):
        if(sym == key.ESCAPE): 
            self.reset()
        if(sym == key.LEFT):
            self.curr = self.curr-1 % len(self.opts)
            self.update_options()
        if(sym == key.RIGHT):
            self.curr = self.curr+1 % len(self.opts)
            self.update_options()
        if(sym == key.SPACE):
            self.dispatch_event("on_select", self.hx, self.opts[self.curr % len(self.opts)])
            self.reset()
        return pyglet.event.EVENT_HANDLED

    def on_open(self, hx, opts):
        self.hx = hx
        self.opts = opts
        px = hx.into_px()
        self.border.position = px + Px(0,TILE_HEIGHT*1.5+PADDING,0)
        self.border.visible = True
        self.curr = 0
        for i in range(len(self.display)):
            pos = px + Px(TILE_WIDTH*(1-i)+PADDING*(1-i),TILE_HEIGHT*1.5+PADDING,0)
            self.guides[i].position = (pos.x,pos.y,pos.z)
            self.guides[i].visible = True
        self.update_options()

    def update_options(self):
        if self.curr is None: return
        px = self.hx.into_px()

        for i in range(len(self.display)):
            pos = px + Px(TILE_WIDTH*(1-i)+PADDING*(1-i),TILE_HEIGHT*1.5+PADDING,0)
            if self.display[i] is not None: self.display[i].delete()
            typ, idx = self.opts[(self.curr+1-i) % len(self.opts)]
            self.display[i] = self.asset_factory.create_sprite(typ, idx, self.batch, pos)
    
    def reset(self):
        self.dispatch_event("on_close")
        self.border.visible = False
        for it in self.guides: it.visible = False
        for i,it in enumerate(self.display): 
            it.delete()
            self.display[i] = None

Overlay.register_event_type('on_close')
Overlay.register_event_type('on_select')