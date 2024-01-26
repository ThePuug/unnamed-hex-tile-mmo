import pyglet
from logging import debug
from pyglet.window import key

from Scene import Scene
from Tile import Px, Tile

PADDING = 10

class Overlay(pyglet.event.EventDispatcher):
    def __init__(self, img, batch, group):
        self.curr = 0

        self.border = pyglet.shapes.Rectangle(0,0,0,0, color=(225, 225, 225, 255), batch=batch, group=group)
        self.border.width = Tile.WIDTH*3 + 4*PADDING
        self.border.height = Tile.HEIGHT + 2*PADDING
        self.border.anchor_x = self.border.width / 2
        self.border.anchor_y = -Tile.HEIGHT
        self.border.visible = False

        self.display = [pyglet.sprite.Sprite(img,batch=batch,group=group) for it in range(3)]
        for it in self.display:
            it.scale_x = Tile.WIDTH/img.width
            it.scale_y = Tile.HEIGHT/(3/4*img.height)
            it.visible = False
    
    def on_key_press(self,sym,mod):
        if(sym == key.ESCAPE): 
            self.border.visible = False
            for it in self.display: it.visible = False
            self.dispatch_event("on_close")
        if(sym == key.LEFT):
            self.curr = self.curr-1 if self.curr > 0 else len(self.opts)-1
            self.update_options()
        if(sym == key.RIGHT):
            self.curr = self.curr+1 if self.curr < len(self.opts)-1 else 0
            self.update_options()
        if(sym == key.SPACE):
            self.dispatch_event("on_select",self.hx,self.opts[self.curr],None)
            self.dispatch_event("on_close")
            self.border.visible = False
            for it in self.display: it.visible = False
        return pyglet.event.EVENT_HANDLED

    def on_open(self,hx,opts):
        debug("{},{}".format(hx,opts))
        self.hx = hx
        self.opts = opts
        px = hx.into_px()
        self.border.position = (px.x,px.y)
        self.border.visible = True

        self.curr = 0
        for i,it in enumerate(self.display):
            it.position = (px.x+Tile.WIDTH*(i-1)+PADDING*(i-1),
                           px.y+Tile.HEIGHT*1.5+PADDING,0)
            it.visible = True
        self.update_options()

    def update_options(self):
        if self.curr is None: return
        self.display[0].image = self.opts[self.curr-1 if self.curr > 0 else len(self.opts)-1]
        self.display[1].image = self.opts[self.curr]
        self.display[2].image = self.opts[self.curr+1 if self.curr < len(self.opts)-1 else 0]

Overlay.register_event_type('on_close')
Overlay.register_event_type('on_select')