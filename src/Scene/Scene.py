import collision
from logging import debug, info, warn
import math
import pyglet

from Config import *
from Event import *
from HxPx import Hx, Px
from Quickle import DECODER
from Scene.Generator import Generator
from StateManager import ACTION_BAR

R=5
NEIGHBORS = [Hx(+1,0,0),Hx(+1,-1,0),Hx(0,-1,0),Hx(-1,0,0),Hx(-1,+1,0),Hx(0,+1,0)]

class Impl(pyglet.event.EventDispatcher):
    def __init__(self, asset_factory, actor_factory, state_manager, batch):
        self.asset_factory = asset_factory
        self.actor_factory = actor_factory
        self.batch = batch
        self.state_manager = state_manager
        self.tiles = {}
        self.actors = {}
        self.decorations = {}
        self.generator = Generator()

    def try_load_actor(self, _, evt): self.dispatch_event("on_do", None, evt)
    def do_load_actor(self, _, evt):
        actor = self.actor_factory.create(evt)
        self.state_manager.push_handlers(actor)
        actor.push_handlers(self.state_manager)
        self.push_handlers(actor)
        actor.push_handlers(self)
        self.actors[evt.id] = actor

    def try_move_actor(self, tid, evt):
        actor = self.actors[evt.actor.id].state

        px = Px(*actor.px)
        hx = px.into_hx()
        heading_hx = hx+Hx(*evt.actor.heading)
        heading_px = heading_hx.into_px()
        heading_offset_px = heading_px-px
        heading_offset_angle = math.atan2(heading_offset_px.y, heading_offset_px.x)
        new_px = px + Px(actor.speed*evt.dt*math.cos(heading_offset_angle), ISO_SCALE*actor.speed*evt.dt*math.sin(heading_offset_angle), 0)

        if actor.air_time is None and not(evt.actor.air_time == 0):
            evt.actor.air_time = None
            evt.actor.air_dz = 0

        if actor.air_time is not None and actor.air_dz > 1:
            it = self.tiles.get(new_px.into_hx()+Hx(0,0,math.floor(actor.air_dz)))
            if(it is not None and it.flags & FLAG_SOLID):
                new_px.z += math.floor(actor.air_dz)
                evt.actor.air_dz = actor.air_dz - math.floor(actor.air_dz)

        if actor.air_dz <= 0:
            if actor.air_time is not None:
                for it in [self.tiles.get(new_px.into_hx()+Hx(0,0,z)) 
                           for z in range(math.ceil(actor.air_dz+evt.dt*actor.speed/(TILE_RISE*2)),math.floor(actor.air_dz),-1)]:
                    if it is not None and it.flags & FLAG_SOLID:
                        new_px.z = it.hx.z
                        evt.actor.air_dz = 0
                        evt.actor.air_time = None
                        break
            else:
                it = self.tiles.get(new_px.into_hx())
                if it is None or not(it.flags & FLAG_SOLID):
                    evt.actor.air_time = actor.vertical*(TILE_RISE*2)/actor.speed
                    evt.actor.air_dz = 0
        
        if evt.actor.air_dz is None: evt.actor.air_dz = actor.air_dz

        collider = collision.Poly(collision.Vector(new_px.x, new_px.y), 
                                    [collision.Vector(it.x, it.y) for it in Px(0,0,0).vertices(7, ORIENTATION_FLAT)], 0)
        response = collision.Response()
        for neighbor in [it+Hx(0,0,z+1+max(0,math.floor(evt.actor.air_dz))) for it in NEIGHBORS for z in range(actor.height)]:
            it = self.tiles.get(hx+neighbor)
            response.reset()
            if it is not None and it.sprite is not None and collision.collide(collider, it.collider, response): 
                if heading_hx == it.hx - Hx(0, 0, it.hx.z-heading_hx.z): 
                    new_px = Px(px.x, px.y, new_px.z)
                    break
                heading_offset_px = heading_px - Px(*it.collider.pos, 0)
                heading_offset_angle = math.atan2(heading_offset_px.y, heading_offset_px.x)
                new_px = px + Px(actor.speed*evt.dt*math.cos(heading_offset_angle), ISO_SCALE*actor.speed*evt.dt*math.sin(heading_offset_angle), 0)
                break

        evt.actor.px = new_px.state
        self.dispatch_event("on_do", tid, evt, True)
    def do_move_actor(self, tid, evt): self.actors[evt.actor.id].state = evt.actor

    def do_unload_actor(self, tid, evt):
        del self.actors[evt.id]

    def try_discover_tile(self, tid, evt):
        c = Hx(*evt.hx)
        for q in range(-R, R+1):
            r1 = max(-R, -q-R)
            r2 = min( R, -q+R)
            for r in range(r1,r2+1):
                hx = c + Hx(q,r,0)
                hx.z = math.floor((self.generator.at(Hx(hx.q,hx.r,-1))/255.0)*20)
                if self.tiles.get(hx) is not None: continue
                tile = self.asset_factory.create_tile("terrain", 1, self.batch, hx.into_px())
                self.dispatch_event("on_do", None, TileChangeEvent(hx.state, tile.state), True)

    def try_change_tile(self, tid, evt): self.dispatch_event("on_do", tid, evt, True)
    def do_change_tile(self, tid, evt):
        hxz = Hx(*evt.hx)
        tile = self.tiles.get(hxz)
        if tile is not None:
            self.tiles[hxz].delete()
            del self.tiles[hxz]
        if evt.tile.sprite__typ is not None and evt.tile.sprite__idx is not None:
            self.tiles[hxz] = self.asset_factory.create_tile(evt.tile.sprite__typ, evt.tile.sprite__idx, self.batch, hxz.into_px())

    def from_file(self):
        tiles = {}
        info("loading scene")
        data = DECODER.loads(pyglet.resource.file("default.0","rb").read())
        for i,it in data.items():
            hx = Hx(*i)
            tile = self.asset_factory.create_tile(it.sprite__typ, it.sprite__idx, self.batch, hx.into_px(), it.flags)
            tiles[hx] = tile
        if len(tiles) == 0: raise Exception("no tiles in scene")
        return tiles
    
    @property
    def state(self):
        return dict([(i.state, it.state) for i,it in self.tiles.items()])

Impl.register_event_type("on_do")
Impl.register_event_type('on_try')

class Scene(Impl):
    def __init__(self, asset_factory, actor_factory, state_manager, batch):
        super().__init__(asset_factory, actor_factory, state_manager, batch)
        # self.tectonics[hx] = pyglet.shapes.Polygon(*[[it.x,it.y] for it in hx.vertices()],color=(it,it,it,255),batch=self.batch)

    def do_load_actor(self, tid, evt):
        super().do_load_actor(tid, evt)
        if evt.id == self.state_manager.tid:
            actor = self.actors[evt.id]
            self.state_manager.actor = actor
            self.state_manager.registry[ACTION_BAR].push_handlers(actor)

    def do_move_actor(self, tid, evt):
        super().do_move_actor(tid, evt)
        self.actors[evt.actor.id].disp_dt += evt.dt

    def do_unload_actor(self, tid, evt):
        self.actors[evt.id].sprite.delete()
        super().do_unload_actor(tid, evt)

    def on_looking_at(self, actor, now, was):
        if self.tiles.get(was) is not None: self.tiles.get(was).sprite.color = (255,255,255)
        it = self.tiles.get(now+Hx(0,0,1))
        if it is None:
            for i in range(R): 
                it = self.tiles.get(now-Hx(0,0,i))
                if it is not None: break
        if it is not None: 
            actor.focus = it.hx
            it.sprite.color = (200,200,100)
        else: self.dispatch_event("on_try", None, TileDiscoverEvent(Hx(now.q,now.r,0).state), True)
    