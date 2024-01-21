import pyglet
from logging import debug
from pyglet.window import key

from Scene import Scene
from Tile import Px

PADDING = 10

class Overlay(pyglet.event.EventDispatcher):
    def __init__(self, scene, batch, group):
        self.scene = scene
        self.curr = 0

        self.border = pyglet.shapes.Rectangle(0,0,0,0, color=(225, 225, 225, 255), batch=batch, group=group)
        self.border.width = Scene.TILE_WIDTH*3 + 4*PADDING
        self.border.height = Scene.TILE_HEIGHT + 2*PADDING
        self.border.anchor_x = self.border.width / 2
        self.border.anchor_y = -Scene.TILE_HEIGHT
        self.border.visible = False

        self.opts = [pyglet.sprite.Sprite(scene.streets[0],batch=batch,group=group) for it in range(3)]
        for it in self.opts:
            it.scale_x = Scene.TILE_WIDTH/83
            it.scale_y = Scene.TILE_HEIGHT/96
            it.visible = False
    
    def on_key_press(self,sym,mod):
        if(sym == key.ESCAPE): 
            self.border.visible = False
            for it in self.opts: it.visible = False
        if(sym == key.LEFT):
            self.curr = self.curr-1 if self.curr > 0 else len(self.scene.streets)-1
            self.update_options()
        if(sym == key.RIGHT):
            self.curr = self.curr+1 if self.curr < len(self.scene.streets)-1 else 0
            self.update_options()
        if(sym == key.B):
            self.dispatch_event("on_change_tile", self.hx, self.scene.streets[0], self.scene.buildings[1])
            self.dispatch_event("on_add_decoration", self.hx, Px(-3*Scene.TILE_WIDTH/6,-3*Scene.TILE_HEIGHT/12,0), self.scene.decorators[0])
            self.dispatch_event("on_add_decoration", self.hx, Px(-2*Scene.TILE_WIDTH/6,-4*Scene.TILE_HEIGHT/12,0), self.scene.decorators[0])
            self.dispatch_event("on_add_decoration", self.hx, Px(-1*Scene.TILE_WIDTH/6,-5*Scene.TILE_HEIGHT/12,0), self.scene.decorators[0])
            self.dispatch_event("on_add_decoration", self.hx, Px(1*Scene.TILE_WIDTH/6,-5*Scene.TILE_HEIGHT/12,0), self.scene.decorators[1])
            self.dispatch_event("on_add_decoration", self.hx, Px(2*Scene.TILE_WIDTH/6,-4*Scene.TILE_HEIGHT/12,0), self.scene.decorators[1])
            self.dispatch_event("on_add_decoration", self.hx, Px(3*Scene.TILE_WIDTH/6,-3*Scene.TILE_HEIGHT/12,0), self.scene.decorators[1])
            self.dispatch_event("on_add_decoration", self.hx, Px(-3*Scene.TILE_WIDTH/6,-1*Scene.TILE_HEIGHT/12,0), self.scene.decorators[0])
            self.dispatch_event("on_add_decoration", self.hx, Px(-2*Scene.TILE_WIDTH/6,-2*Scene.TILE_HEIGHT/12,0), self.scene.decorators[0])
            self.dispatch_event("on_add_decoration", self.hx, Px(-1*Scene.TILE_WIDTH/6,-3*Scene.TILE_HEIGHT/12,0), self.scene.decorators[0])
            self.dispatch_event("on_add_decoration", self.hx, Px(1*Scene.TILE_WIDTH/6,-3*Scene.TILE_HEIGHT/12,0), self.scene.decorators[1])
            self.dispatch_event("on_add_decoration", self.hx, Px(2*Scene.TILE_WIDTH/6,-2*Scene.TILE_HEIGHT/12,0), self.scene.decorators[1])
            self.dispatch_event("on_add_decoration", self.hx, Px(3*Scene.TILE_WIDTH/6,-1*Scene.TILE_HEIGHT/12,0), self.scene.decorators[1])
            self.border.visible = False
            for it in self.opts: it.visible = False
        if(sym == key.SPACE):
            self.dispatch_event("on_change_tile",self.hx,self.scene.streets[self.curr],None)
            self.border.visible = False
            for it in self.opts: it.visible = False

    def on_build(self,hx):
        self.hx = hx
        px = hx.into_px(Scene.TILE_SIZE)
        self.border.position = (px.x,px.y)
        self.border.visible = True

        tile = self.scene.tiles.get(hx)
        for i,it in enumerate(self.scene.streets):
            if it == tile.image: self.curr = i
        for i,it in enumerate(self.opts):
            it.position = (px.x+Scene.TILE_WIDTH*(i-1)+PADDING*(i-1),
                           px.y+Scene.TILE_HEIGHT*1.5+PADDING,0)
            it.visible = True
        self.update_options()

    def update_options(self):
        if self.curr is None: return
        self.opts[0].image = self.scene.streets[self.curr-1 if self.curr > 0 else len(self.scene.streets)-1]
        self.opts[1].image = self.scene.streets[self.curr]
        self.opts[2].image = self.scene.streets[self.curr+1 if self.curr < len(self.scene.streets)-1 else 0]

Overlay.register_event_type('on_change_tile')
Overlay.register_event_type('on_add_decoration')