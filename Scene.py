from logging import debug
from math import sqrt
import math
import pyglet

from Tile import Hx, Tile

class Scene(pyglet.event.EventDispatcher):
    R=5

    def __init__(self, streets, buildings, decorators, batch, groups):
        self.streets = streets
        self.buildings = buildings
        self.decorators = decorators
        self.batch = batch
        self.groups = groups
        self.tiles = {}
        self.decorations = {}

    def on_looking_at(self, hx):
        if self.tiles.get(hx,None) is None:
            self.dispatch_event('on_discover',hx)

    def on_add_decoration(self, hx, offset, decorator):
        new_hx = Hx(hx.q,hx.r,hx.z+1)
        new_tile = self.create_tile(new_hx,decorator, (Tile.WIDTH/6 / decorator.width, Tile.HEIGHT/4 / decorator.height))
        new_tile.x += offset.x
        new_tile.y += offset.y
        if(self.decorations.get(new_hx,None) is None): self.decorations[new_hx] = []
        self.decorations[new_hx].append(new_tile)

    def on_change_tile(self, hx, background, above):
        self.tiles.get(hx).image = background

        abhx = Hx(hx.q,hx.r,hx.z+2)
        ab = self.tiles.get(abhx,None)
        if above is not None:
            if ab is None: self.tiles[abhx] = self.create_tile(abhx,above)
            else: ab.image = above
        elif above is None and ab is not None: 
            ab.image.delete()
            del self.tiles[abhx]

    def on_discover(self, c):
        for q in range(-Scene.R, Scene.R+1):
            r1 = max(-Scene.R, -q-Scene.R)
            r2 = min( Scene.R, -q+Scene.R)
            for r in range(r1,r2+1):
                hx = Hx(c.q + q, c.r + r, c.z)
                if not(self.tiles.get(hx,None) is None): continue
                self.tiles[hx] = self.create_tile(hx,self.streets[0])
    
    def create_tile(self,hx,img,scale = None):
        px = hx.into_px()
        new = pyglet.sprite.Sprite(img=img,batch=self.batch,group=self.groups[hx.z])
        if scale is None:
            new.scale_x = Tile.WIDTH / (img.width-2)
            new.scale_y = Tile.HEIGHT / (3/4*img.height-2)
        else:
            new.scale_x = scale[0]
            new.scale_y = scale[1]
        new.x = px.x
        new.y = px.y + Tile.HEIGHT/4 + (px.z//2)*(Tile.RISE)
        return new


Scene.register_event_type('on_discover')