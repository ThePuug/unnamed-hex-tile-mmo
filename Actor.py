import collision
import pyglet
from pyglet.window import key
from pyglet.math import Vec2

from Config import *
from Event import *
from HxPx import Hx, Px
from Asset import DepthSprite, depth_shader

DEFAULT_SPEED = 120
DEFAULT_VERTICAL = 1.33
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

class Impl(pyglet.event.EventDispatcher):
    def __init__(self, evt):
        self.id = evt.id
        self.last_clock = 0
        self.heading = Hx(0,0,0)
        self.air_dz = 0
        self.air_time = None
        self.focus = Hx(0,0,0)
        self.height = DEFAULT_HEIGHT
        self.speed = DEFAULT_SPEED
        self.vertical = DEFAULT_VERTICAL
        self.px = Px(*(evt.pos if evt.pos is not None else (0,0,0)))
        self.collider = collision.Poly(collision.Vector(self.px.x, self.px.y), 
                                       [collision.Vector(it.x, it.y) for it in Px(0,0,0).vertices(7, ORIENTATION_FLAT)], 0)

    @property
    def hx(self): return self._hx

    @hx.setter
    def hx(self, v):
        self._hx = v
        self._px = v.into_px()
        self.recalc()

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
                                  px=self.px.state)
    
    @state.setter
    def state(self, v):
        self.px = Px(*v.px)
        self.collider.pos = collision.Vector(*v.px[:2])
        self.heading = Hx(*v.heading)
        self.air_dz = v.air_dz
        self.air_time = v.air_time  
        self.last_clock = v.last_clock


Impl.register_event_type('on_try')
Impl.register_event_type('on_looking_at')

class Actor(Impl):
    def __init__(self, evt, key_state_handler, batch):
        self.key_state = key_state_handler

        frames_blank = pyglet.image.ImageGrid(pyglet.resource.image("blank.png"), rows=4, columns=4)
        for it in frames_blank:
            it.anchor_x = 31
            it.anchor_y = 5
        self.animations = {
            "walk_n": pyglet.image.Animation.from_image_sequence([frames_blank[i] for i in [1,2,3,0]], duration=0.4),
            "walk_e": pyglet.image.Animation.from_image_sequence([frames_blank[i] for i in [5,6,7,4]], duration=0.4),
            "walk_w": pyglet.image.Animation.from_image_sequence([frames_blank[i] for i in [9,10,11,8]], duration=0.4),
            "walk_s": pyglet.image.Animation.from_image_sequence([frames_blank[i] for i in [13,14,15,12]], duration=0.4),
            "stand_n": pyglet.image.Animation.from_image_sequence([frames_blank[i] for i in [0,2]], duration=0.4),
            "stand_e": pyglet.image.Animation.from_image_sequence([frames_blank[i] for i in [4,6]], duration=0.4),
            "stand_w": pyglet.image.Animation.from_image_sequence([frames_blank[i] for i in [8,10]], duration=0.4),
            "stand_s": pyglet.image.Animation.from_image_sequence([frames_blank[i] for i in [12,14]], duration=0.4)}
        self.sprite = DepthSprite(self.animations["walk_s"], batch=batch, program=depth_shader)
        self.sprite.scale = 1
        super().__init__(evt)

    def on_action(self, evt, hx, *args):
        if(evt == "on_overlay"): self.dispatch_event(evt, self.hx+self.heading+hx, *args)

    def do_move_actor(self, _, evt):
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
        if actor.air_time is not None: 
            super().update(actor, dt)
            self.dispatch_event('on_try', actor.id, ActorMoveEvent(actor=actor, dt=dt))
        else:
            if self.key_state[key.SPACE]: 
                actor.air_time = 0
                self.dispatch_event('on_try', actor.id, ActorMoveEvent(actor=actor, dt=dt))
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
                self.dispatch_event('on_try', actor.id, ActorMoveEvent(actor=actor, dt=dt))
        self.recalc()

    def recalc(self):
        super().recalc()
        self.sprite.position = self._px.into_screen((0,self.air_dz*TILE_RISE,self.air_dz+self.height))

Actor.register_event_type('on_overlay')

class Factory:
    def __init__(self, key_state_handler, batch):
        self.key_state_handler = key_state_handler
        self.batch = batch

    def create(self, evt):
        return Actor(evt, self.key_state_handler, self.batch)

class ImplFactory:
    def create(self, evt):
        return Impl(evt)        