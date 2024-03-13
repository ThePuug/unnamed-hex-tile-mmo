import collision
import pyglet
from pyglet.window import key
from pyglet.math import Vec2

from Config import *
from Event import *
from HxPx import Hx, Px

DEFAULT_SPEED = 120
DEFAULT_VERTICAL = 1.2
DEFAULT_HEIGHT = 3

class State(quickle.Struct):
    id: int
    height: int
    heading: tuple
    speed: int
    last_clock: float
    vertical: float
    air_dz: float
    air_time: float
    px: tuple
    typ: str
    busy: False

class Impl(pyglet.event.EventDispatcher):
    def __init__(self, id, typ, px, behaviour):
        self.id = id
        self.typ = typ
        self.behaviour = behaviour
        self.last_clock = 0
        self.heading = Hx(0,0,0)
        self.air_dz = 0
        self.air_time = None
        self.focus = Hx(0,0,0)
        self.height = DEFAULT_HEIGHT
        self.speed = DEFAULT_SPEED
        self.vertical = DEFAULT_VERTICAL
        self.px = Px(*(px[:3]))
        self.busy = False
        self.collider = collision.Poly(collision.Vector(self.px.x, self.px.y), 
                                       [collision.Vector(it.x, it.y) for it in Px(0,0,0).vertices(7, ORIENTATION_FLAT)], 0)

    @property
    def hx(self): return self._hx

    @property
    def px(self): return self._px

    @px.setter
    def px(self, v):
        self._px = v
        self._hx = v.into_hx()
        self.recalc()

    def update(self, actor, dt):
        if actor.air_time is not None:
            actor.air_time += dt
            if (actor.air_time*actor.speed/(TILE_RISE*2))/actor.vertical > 1: actor.air_dz -= actor.speed/(TILE_RISE*2)*dt
            else: actor.air_dz = Vec2(0,0).lerp(Vec2(0,actor.vertical), (actor.air_time*actor.speed/(TILE_RISE*2))/actor.vertical).y
            self.dispatch_event('on_try', self.id, ActorMoveEvent(actor, dt), False)
        
        elif self.behaviour is not None:
            self.behaviour.update(self, dt)

    def recalc(self):
        was_focus_hx = self.focus
        self.focus = self.hx+self.heading
        now_focus_hx = self.hx+self.heading
        if now_focus_hx != was_focus_hx: self.dispatch_event('on_looking_at', self, now_focus_hx, was_focus_hx)

    @property
    def state(self): return State(id=self.id, 
                                  height=self.height, 
                                  heading=self.heading.state, 
                                  speed=self.speed, 
                                  last_clock=self.last_clock,
                                  vertical=self.vertical, 
                                  air_dz=self.air_dz, 
                                  air_time=self.air_time, 
                                  px=self.px.state,
                                  typ=self.typ,
                                  busy=self.busy)
    
    @state.setter
    def state(self, v):
        self.px = Px(*v.px)
        self.collider.pos = collision.Vector(*v.px[:2])
        self.heading = Hx(*v.heading)
        self.air_dz = v.air_dz
        self.air_time = v.air_time  
        self.last_clock = v.last_clock
        self.typ = v.typ
        self.busy = v.busy

Impl.register_event_type('on_try')
Impl.register_event_type('on_looking_at')

class Actor(Impl):
    def __init__(self, id, typ, px, key_state_handler, asset_factory):
        self.key_state = key_state_handler
        self.disp_dt = 0
        self.disp_pos = Px(0,0,0)
        sprite, anims = asset_factory.create_actor(typ)
        self.sprite = sprite
        self.animations = anims
        super().__init__(id, typ, px, None)

    def on_action(self, evt, hx, *args):
        if(evt == "on_overlay"): self.dispatch_event(evt, self.hx+self.heading+hx, *args)

    def do_move_actor(self, tid, evt):
        if evt.actor.id != self.id: return
        heading = Hx(*evt.actor.heading)
        if heading == Hx(0,0,0):
            if self.sprite.image == self.animations["walk_n"]: self.sprite.image = self.animations["stand_n"]
            if self.sprite.image == self.animations["walk_e"]: self.sprite.image = self.animations["stand_e"]
            if self.sprite.image == self.animations["walk_w"]: self.sprite.image = self.animations["stand_w"]
            if self.sprite.image == self.animations["walk_s"]: self.sprite.image = self.animations["stand_s"]
        else:
            if heading.r == -1:
                if self.sprite.image != self.animations["walk_n"]: 
                    self.sprite.image = self.animations["walk_n"]
            elif heading.r == +1:
                if self.sprite.image != self.animations["walk_s"]:
                    self.sprite.image = self.animations["walk_s"]
            elif heading.q == -1:
                if self.sprite.image != self.animations["walk_w"] and not(self.key_state[key.UP] or self.key_state[key.DOWN]): 
                    self.sprite.image = self.animations["walk_w"]
            elif heading.q == +1:
                if self.sprite.image != self.animations["walk_e"] and not(self.key_state[key.UP] or self.key_state[key.DOWN]): 
                    self.sprite.image = self.animations["walk_e"]

    def update(self, actor, dt):
        if actor.id != self.id: return
        if actor.air_time is not None: 
            actor.air_time += dt
            if (actor.air_time*actor.speed/(TILE_RISE*2))/actor.vertical > 1: actor.air_dz -= actor.speed/(TILE_RISE*2)*dt
            else: actor.air_dz = Vec2(0,0).lerp(Vec2(0,actor.vertical), (actor.air_time*actor.speed/(TILE_RISE*2))/actor.vertical).y
        else:
            if self.key_state[key.SPACE]: 
                actor.air_time = 0
                self.dispatch_event('on_try', actor.id, ActorMoveEvent(actor=actor, dt=dt), False)
            elif self.key_state[key.LEFT] or self.key_state[key.RIGHT] or self.key_state[key.UP] or self.key_state[key.DOWN]:
                heading = Hx(*actor.heading)
                if self.key_state[key.UP]: 
                    if self.key_state[key.LEFT] or not self.key_state[key.RIGHT] and (heading == Hx(-1,0,0) or 
                                                                                      heading == Hx(-1,+1,0) or 
                                                                                      heading == Hx(+1,-1,0)): heading = Hx(-1,+1,0)
                    else: heading = Hx(0,+1,0)
                elif self.key_state[key.DOWN]: 
                    if self.key_state[key.RIGHT] or not self.key_state[key.LEFT] and (heading == Hx(1,0,0) or 
                                                                                      heading == Hx(+1,-1,0) or 
                                                                                      heading == Hx(-1,1,0)): heading = Hx(+1,-1,0)
                    else: heading = Hx(0,-1,0)
                elif self.key_state[key.RIGHT]: heading = Hx(+1,+0,0)            
                elif self.key_state[key.LEFT]: heading = Hx(-1,+0,0)

                actor.heading = heading.state
                self.dispatch_event('on_try', actor.id, ActorMoveEvent(actor=actor, dt=dt), False)

    def recalc(self):
        super().recalc()
        if self.disp_dt <= 0: self.disp_pos = self.px.into_screen((0, self.air_dz*TILE_RISE, 1+self.height+self.air_dz))
        self.sprite.position = self.disp_pos[:3]

Actor.register_event_type('on_overlay')

class Factory:
    def __init__(self, key_state_handler, asset_factory):
        self.key_state_handler = key_state_handler
        self.asset_factory = asset_factory

    def create(self, id, typ, px): return Actor(id, typ, px, self.key_state_handler, self.asset_factory)

class ImplFactory:
    def __init__(self, behaviour_factory):
        self.behaviour_factory = behaviour_factory

    def create(self, id, typ, px, **kwargs): 
        behaviour = kwargs.pop("behaviour", typ)
        return Impl(id, typ, px, self.behaviour_factory.create(behaviour))