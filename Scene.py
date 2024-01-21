from logging import debug
from math import sqrt
import math
import pyglet

from Tile import Hx

class Scene(pyglet.event.EventDispatcher):
    TILE_SIZE=48
    TILE_WIDTH=math.ceil(sqrt(3)*TILE_SIZE)
    TILE_HEIGHT=math.ceil(TILE_SIZE*2)
    TILE_RISE=math.ceil(3*TILE_SIZE/4)
    R=5

    def __init__(self, streets, buildings, decorators, batch, groups):
        self.streets = streets
        self.buildings = buildings
        self.decorators = decorators
        self.scale_x = Scene.TILE_WIDTH/81
        self.scale_y = Scene.TILE_HEIGHT/70
        self.batch = batch
        self.groups = groups
        self.tiles = {}
        self.decorations = {}

    def on_looking_at(self, hx):
        if self.tiles.get(hx,None) is None:
            self.dispatch_event('on_discover',hx)

    def on_add_decoration(self, hx, offset, decorator):
        new_hx = hx.offset(0,0,1)
        new_tile = self.create_tile(new_hx,decorator)
        new_tile.x += offset.x
        new_tile.y += offset.y
        if(self.decorations.get(new_hx,None) is None): self.decorations[new_hx] = []
        self.decorations[new_hx].append(new_tile)

    def on_change_tile(self, hx, background, above):
        self.tiles.get(hx).image = background

        abhx = hx.offset(0,0,2)
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
    
    def create_tile(self,hx,img):
        px = hx.into_px(Scene.TILE_SIZE)
        new = pyglet.sprite.Sprite(img=img,batch=self.batch,group=self.groups[hx.z])
        new.scale_x = self.scale_x
        new.scale_y = self.scale_y
        new.x = px.x
        new.y = px.y + Scene.TILE_SIZE/2 + (px.z//2)*(Scene.TILE_HEIGHT/3)
        return new


Scene.register_event_type('on_discover')