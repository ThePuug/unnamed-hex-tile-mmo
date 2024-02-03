import collision
import pyglet

from Config import *
from HxPx import Hx

R=5
NEIGHBORS = [Hx(+1,0,0),Hx(+1,-1,0),Hx(0,-1,0),Hx(-1,0,0),Hx(-1,+1,0),Hx(0,+1,0)]

class Scene(pyglet.event.EventDispatcher):

    def __init__(self, tile_factories, batch):
        self.terrain = tile_factories.terrain
        self.buildings = tile_factories.buildings
        self.decorators = tile_factories.decorators
        self.batch = batch
        self.tiles = {}
        self.decorations = {}
        self.on_discover(Hx(0,0,0))

    def on_looking_at(self, actor, now, was):
        if self.tiles.get(was) is not None: self.tiles.get(was).sprite.color = (255,255,255)
        it = self.tiles.get(now+Hx(0,0,1))
        if it is None:
            for i in range(5): 
                it = self.tiles.get(now-Hx(0,0,i))
                if it is not None: break
        if it is not None: 
            actor.focus = it.hx
            it.sprite.color = (200,200,100)
        elif now.z < 5: self.dispatch_event('on_discover',Hx(now.q,now.r,0))

    def on_select(self, hx, factory):
        hxz = Hx(hx.q,hx.r,hx.z)
        tile = self.tiles.get(hxz)
        if tile is not None:
            self.tiles[hxz].delete()
            del self.tiles[hxz]
        self.tiles[hxz] = factory.create(hxz,self.batch)

    def on_discover(self, c):
        for q in range(-R, R+1):
            r1 = max(-R, -q-R)
            r2 = min( R, -q+R)
            for r in range(r1,r2+1):
                hx = Hx(c.q + q, c.r + r, c.z)
                if not(self.tiles.get(hx,None) is None): continue
                self.tiles[hx] = self.terrain[0].create(hx,self.batch)
    
    def on_move_to(self, actor, px):
        hx = px.into_hx()
        tile = self.tiles.get(hx)
        if(tile is not None and tile.flags & FLAG_SOLID):
            actor.collider.pos = collision.Vector(px.x,px.y)
            response = collision.Response()
            for it in [self.tiles.get(hx+it+Hx(0,0,z+1)) for it in NEIGHBORS for z in range(actor.height)]:
                response.reset()
                if it is not None and it.sprite is not None and collision.collide(actor.collider,it.collider,response):
                    return
            actor.px = px

Scene.register_event_type('on_discover')