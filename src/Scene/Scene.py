import random
import collision
from logging import info, warning
import math
import pyglet

from Config import *
from Event import *
from HxPx import Hx, Px
from LogId import LOGID
from Quickle import DECODER
from StateManager import ACTION_BAR

R=5
NEIGHBORS = [Hx(+1,0,0),Hx(+1,-1,0),Hx(0,-1,0),Hx(-1,0,0),Hx(-1,+1,0),Hx(0,+1,0)]

class Impl(pyglet.event.EventDispatcher):
    def __init__(self, actor_factory, asset_factory, state_manager, generator):
        self.actor_factory = actor_factory
        self.asset_factory = asset_factory
        self.state_manager = state_manager
        self.tiles = {}
        self.pcs = {}
        self.npcs = {}
        self.decorations = {}
        self.generator = generator

    def try_load_actor(self, tid, evt): self.dispatch_event("on_do", None, evt)
    def do_load_actor(self, tid, evt):
        actors = self.pcs if evt.actor.typ == "blank" else self.npcs
        while evt.actor.id is None:
            id = random.randint(0, pow(2,32)-1)
            if actors.get(id, None) is None: evt.actor.id = id
        actor = self.actor_factory.create(evt.actor.id, evt.actor.typ, evt.actor.px)
        self.state_manager.push_handlers(actor)
        actor.push_handlers(self.state_manager)
        self.push_handlers(actor)
        actor.push_handlers(self)
        actors[evt.actor.id] = actor

    def try_move_actor(self, tid, evt):
        actors = self.pcs if evt.actor.typ == "blank" else self.npcs
        actor = actors[evt.actor.id]
        if actor is None: warning("{:} - Actor not found: {}".format(LOGID.NF_ACTOR, evt.actor.id))
        state = actor.state

        px = Px(*state.px)
        hx = px.into_hx()
        heading_hx = hx+Hx(*evt.actor.heading)
        heading_px = heading_hx.into_px()
        heading_offset_px = heading_px-px
        heading_offset_angle = math.atan2(heading_offset_px.y, heading_offset_px.x)
        new_px = px + Px(state.speed*evt.dt*math.cos(heading_offset_angle), ISO_SCALE*state.speed*evt.dt*math.sin(heading_offset_angle), 0)

        if state.air_time is None and not(evt.actor.air_time == 0):
            evt.actor.air_time = None
            evt.actor.air_dz = 0

        if state.air_time is not None and state.air_dz > 1:
            it = self.tiles.get(new_px.into_hx()+Hx(0,0,math.floor(state.air_dz)))
            if(it is not None and it.flags & FLAG_SOLID):
                new_px.z += math.floor(state.air_dz)
                evt.actor.air_dz = state.air_dz - math.floor(state.air_dz)

        if state.air_dz <= 0:
            if state.air_time is not None:
                for it in [self.tiles.get(new_px.into_hx()+Hx(0,0,z)) 
                           for z in range(math.ceil(state.air_dz+evt.dt*state.speed/(TILE_RISE*2)),math.floor(state.air_dz),-1)]:
                    if it is not None and it.flags & FLAG_SOLID:
                        new_px.z = it.hx.z
                        evt.actor.air_dz = 0
                        evt.actor.air_time = None
                        break
            else:
                it = self.tiles.get(new_px.into_hx())
                if it is None or not(it.flags & FLAG_SOLID):
                    evt.actor.air_time = state.vertical*(TILE_RISE*2)/state.speed
                    evt.actor.air_dz = 0
        
        if evt.actor.air_dz is None: evt.actor.air_dz = state.air_dz

        collider = collision.Poly(collision.Vector(new_px.x, new_px.y), 
                                    [collision.Vector(it.x, it.y) for it in Px(0,0,0).vertices(7, ORIENTATION_FLAT)], 0)
        response = collision.Response()
        for neighbor in [it+Hx(0,0,z+1+max(0,math.floor(evt.actor.air_dz))) for it in NEIGHBORS for z in range(state.height)]:
            it = self.tiles.get(hx+neighbor)
            response.reset()
            if it is not None and it.sprite is not None and collision.collide(collider, it.collider, response): 
                if heading_hx == it.hx - Hx(0, 0, it.hx.z-heading_hx.z): 
                    new_px = Px(px.x, px.y, new_px.z)
                    break
                heading_offset_px = heading_px - Px(*it.collider.pos, 0)
                heading_offset_angle = math.atan2(heading_offset_px.y, heading_offset_px.x)
                new_px = px + Px(state.speed*evt.dt*math.cos(heading_offset_angle), ISO_SCALE*state.speed*evt.dt*math.sin(heading_offset_angle), 0)
                break

        evt.actor.px = new_px.state
        self.dispatch_event("on_do", tid, evt, True)
    def do_move_actor(self, tid, evt): 
        actors = self.pcs if evt.actor.typ == "blank" else self.npcs
        actor = actors.get(evt.actor.id,None)
        if actor is None: warning("{:} - Actor not found: {}".format(LOGID.NF_ACTOR, evt.actor.id))
        else: actor.state = evt.actor

    def try_unload_actor(self, tid, evt): self.dispatch_event("on_do", tid, evt, True)
    def do_unload_actor(self, tid, evt):
        actors = self.pcs if evt.actor.typ == "blank" else self.npcs
        actor = actors.get(evt.actor.id,None)
        if actor is None: warning("{:} - Actor not found: {}".format(LOGID.NF_ACTOR, evt.actor.id))
        else: del actors[evt.actor.id]

    def try_discover_tile(self, tid, evt):
        c = Hx(*evt.hx)
        for q in range(-R, R+1):
            r1 = max(-R, -q-R)
            r2 = min( R, -q+R)
            for r in range(r1,r2+1):
                hx = c + Hx(q,r,0)
                
                hx.z = self.generator.elevation(Hx(hx.q,hx.r,0))
                if self.tiles.get(hx) is not None: continue
                tile = self.asset_factory.create_tile("biomes", 1 if hx.z < 50 else 3 if hx.z < 75 else 5, hx.into_px())
                self.dispatch_event("on_do", None, TileChangeEvent(hx.state, tile.state), True)

                if random.randint(0,100) == 100:
                    self.dispatch_event("on_do", None, ActorLoadEvent(self.actor_factory.create(None, "dog", hx.into_px()).state), True)

                hx.z += 1
                if self.generator.vegetation(Hx(hx.q,hx.r,0)) > 66:
                    tile = self.asset_factory.create_tile("decorators", 0, hx.into_px())
                    self.dispatch_event("on_do", None, TileChangeEvent(hx.state, tile.state), True)
                
    def try_change_tile(self, tid, evt): self.dispatch_event("on_do", tid, evt, True)
    def do_change_tile(self, tid, evt):
        hxz = Hx(*evt.hx)
        tile = self.tiles.get(hxz)
        if tile is not None:
            self.tiles[hxz].delete()
            del self.tiles[hxz]
        if evt.tile is not None:
            self.tiles[hxz] = self.asset_factory.create_tile(evt.tile.sprite__typ, evt.tile.sprite__idx, hxz.into_px())

    def from_file(self):
        tiles = {}
        info("loading scene")
        data = DECODER.loads(pyglet.resource.file("default.0","rb").read())
        for i,it in data.items():
            hx = Hx(*i)
            tile = self.asset_factory.create_tile(it.sprite__typ, it.sprite__idx, hx.into_px(), it.flags)
            tiles[hx] = tile
        if len(tiles) == 0: raise Exception("no tiles in scene")
        info("{} tiles loaded".format(len(tiles)))
        return tiles
    
    @property
    def state(self):
        return dict([(i.state, it.state) for i,it in self.tiles.items()])

Impl.register_event_type("on_do")
Impl.register_event_type('on_try')

class Scene(Impl):
    def __init__(self, actor_factory, asset_factory, state_manager):
        super().__init__(actor_factory, asset_factory, state_manager, None)

    def do_load_actor(self, tid, evt):
        super().do_load_actor(tid, evt)
        actors = self.pcs if evt.actor.typ == "blank" else self.npcs
        actor = actors.get(evt.actor.id,None)
        if actor is None: warning("{:} - Actor not found: {}".format(LOGID.NF_ACTOR, evt.actor.id))
        elif evt.actor.id == self.state_manager.tid:
            self.state_manager.actor = actor
            self.state_manager.registry[ACTION_BAR].push_handlers(actor)

    def do_move_actor(self, tid, evt):
        super().do_move_actor(tid, evt)
        actors = self.pcs if evt.actor.typ == "blank" else self.npcs
        actor = actors.get(evt.actor.id,None)
        if actor is None: warning("{:} - Actor not found: {}".format(LOGID.NF_ACTOR, evt.actor.id))
        else: actor.disp_dt += evt.dt

    def do_unload_actor(self, tid, evt):
        actor = self.pcs.get(evt.id,None)
        if actor is None: warning("{:} - Actor not found: {}".format(LOGID.NF_ACTOR, evt.id))
        else: actor.sprite.delete()
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
    