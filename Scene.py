from logging import debug
import pyglet

from HxPx import Hx
from Tile import Tile

R=5
NEIGHBORS = [Hx(+1,0,0),Hx(+1,-1,0),Hx(0,-1,0),Hx(-1,0,0),Hx(-1,+1,0),Hx(0,+1,0)]

class Scene(pyglet.event.EventDispatcher):

    def __init__(self, assets, batch, groups):
        self.terrain = assets.terrain
        self.streets = assets.streets
        self.buildings = assets.buildings
        self.decorators = assets.decorators
        self.batch = batch
        self.groups = groups
        self.tiles = {}
        self.decorations = {}

    def on_looking_at(self, hx):
        if self.tiles.get(hx,None) is None:
            self.dispatch_event('on_discover',hx)

    def on_select(self, hx, layerset):
        for i,it in enumerate(layerset.layers):
            if it is None: continue
            hxi = Hx(hx.q,hx.r,hx.z+i)
            tile = self.tiles.get(hxi)
            sprite = layerset.into_sprite(i,self.batch)
            if tile is not None:
                self.tiles[hxi].sprite.delete()
                del self.tiles[hxi]
            self.tiles[hxi] = Tile(hxi,sprite,self.groups[hxi.z])

    def on_discover(self, c):
        for q in range(-R, R+1):
            r1 = max(-R, -q-R)
            r2 = min( R, -q+R)
            for r in range(r1,r2+1):
                hx = Hx(c.q + q, c.r + r, c.z)
                if not(self.tiles.get(hx,None) is None): continue
                self.tiles[hx] = Tile(hx, self.terrain[0].into_sprite(0,self.batch), self.groups[hx.z])
    
    def on_move_to(self, actor, px):
        hx = px.into_hx()
        for it in [self.tiles.get(Hx(hx.q+offset.q,hx.r+offset.r,hx.z)) for offset in NEIGHBORS + [hx]]:
            if it is not None:
                debug("{},{}->{},{}".format(it.sprite.x, it.sprite.y, it.sprite.width, it.sprite.height))
    
Scene.register_event_type('on_discover')