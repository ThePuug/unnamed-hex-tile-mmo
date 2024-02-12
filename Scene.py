import collision
from copy import copy
from logging import debug, info, warn
import math
import pickle
import pyglet

from Config import *
from Event import *
from HxPx import Hx, Px
from StateManager import ACTION_BAR

R=5
NEIGHBORS = [Hx(+1,0,0),Hx(+1,-1,0),Hx(0,-1,0),Hx(-1,0,0),Hx(-1,+1,0),Hx(0,+1,0)]

class Impl(pyglet.event.EventDispatcher):
    def __init__(self, asset_factory, actor_factory, state_manager, batch):
        self.asset_factory = asset_factory
        self.actor_factory = actor_factory
        self.batch = batch
        self.state_manager = state_manager
        self.actors = {}
        self.decorations = {}
        self.tiles = self.from_file()
        self.dispatch_event("on_discover", Hx(0,0,0))

    def try_load_actor(self, _, evt): 
        self.dispatch_event("on_do", None, evt)

    def do_load_actor(self, _, evt):
        actor = self.actor_factory.create(evt)
        self.state_manager.push_handlers(actor)
        actor.push_handlers(self.state_manager)
        self.push_handlers(actor)
        actor.push_handlers(self)
        self.actors[evt.id] = actor

    def try_move_actor(self, _, evt):
        actor = self.actors[evt.id]

        if(actor.last_clock + evt.dt > pyglet.clock._time.time()): 
            warn("dt too big")
            return
        self.last_clock = pyglet.clock._time.time()

        heading_hx = actor.hx+Hx(*evt.heading)
        heading_px = heading_hx.into_px()
        offset_px = heading_px-actor.px
        angle = math.atan2(offset_px.y, offset_px.x)
        new_px = actor.px + Px(actor.speed*evt.dt*math.cos(angle), ISO_SCALE*actor.speed*evt.dt*math.sin(angle), 0)
        
        tile = self.tiles.get(actor.hx)
        if(tile is not None and tile.flags & FLAG_SOLID):
            collider = collision.Poly(collision.Vector(new_px.x, new_px.y), 
                                      [collision.Vector(it.x, it.y) for it in Px(0,0,0).vertices(7, ORIENTATION_FLAT)], 0)
            response = collision.Response()
            for hx in [it+Hx(0,0,z+1) for it in NEIGHBORS for z in range(actor.height)]:
                it = self.tiles.get(actor.hx+hx)
                response.reset()
                if it is not None and it.sprite is not None and collision.collide(collider, it.collider, response): 
                    if heading_hx == it.hx - Hx(0, 0, it.hx.z-heading_hx.z): return
                    offset_px = heading_px - Px(*it.collider.pos, 0)
                    angle = math.atan2(offset_px.y, offset_px.x)
                    new_px = actor.px + Px(actor.speed*evt.dt*math.cos(angle), ISO_SCALE*actor.speed*evt.dt*math.sin(angle), 0)
                    break
            evt.pos = (new_px.x, new_px.y, new_px.z)
            self.dispatch_event("on_do", None, evt)
    
    def do_move_actor(self, _, evt):
        actor = self.actors[evt.id]
        actor.px = Px(*evt.pos)

    def do_unload_actor(self, _, evt):
        del self.actors[evt.id]

    def to_file(self):
        info("saving scene")
        try:
            pickle.dump(self.tiles,pyglet.resource.file("default.0",'wb'))
        except Exception as e:
            debug(e)

    def from_file(self):
        try:
            tiles = {}
            data = pickle.load(pyglet.resource.file("default.0","rb"))
            for hx,it in data.items():
                tile = self.asset_factory.create_tile(it.sprite["texture"]["typ"], it.sprite["texture"]["idx"], self.batch, hx.into_px(), it.flags)
                tiles[hx] = tile
            return tiles
        except Exception as e:
            debug(e)
            return {}

Impl.register_event_type("on_do")
Impl.register_event_type('on_try')

class Scene(Impl):

    def do_load_actor(self, tid, evt):
        super().do_load_actor(tid, evt)
        if evt.id == self.state_manager.tid:
            self.state_manager.actor = self.actors[evt.id]
            self.state_manager.registry[ACTION_BAR].push_handlers(self.actors[evt.id])

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

    def on_select(self, hx, asset):
        typ, idx = asset
        hxz = Hx(hx.q,hx.r,hx.z)
        tile = self.tiles.get(hxz)
        if tile is not None:
            self.tiles[hxz].delete()
            del self.tiles[hxz]
        self.tiles[hxz] = self.asset_factory.create_tile(typ, idx, self.batch, hxz.into_px())

    def on_discover(self, c):
        for q in range(-R, R+1):
            r1 = max(-R, -q-R)
            r2 = min( R, -q+R)
            for r in range(r1,r2+1):
                hx = Hx(c.q + q, c.r + r, c.z)
                if not(self.tiles.get(hx,None) is None): continue
                self.tiles[hx] = self.asset_factory.create_tile("terrain", 0, self.batch, hx.into_px())
    
Scene.register_event_type('on_discover')
