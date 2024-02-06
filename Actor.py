import collision
import copy
import math
import pyglet
from pyglet.window import key

from Config import *
from Event import ActorMoveEvent
from HxPx import Hx, Px
from Assets import DepthSprite, depth_shader

DEFAULT_SPEED = 90

class Impl(pyglet.event.EventDispatcher):
    def __init__(self, id):
        self.id = id
        self.last_clock = 0

    def on_move_to(self, e):
        # avoid speed hack by sending a large dt
        if(self.last_clock + e.dt > pyglet.clock._time): return
        self.last_clock = pyglet.clock._time
        self.dispatch_event("do_actor_move", e.id, Px(*e[:3]))

Impl.register_event_type('do_actor_move')

class Actor(Impl):
    def __init__(self, id, key_state_handler, batch):
        super().__init__(id)
        self.key_handler = key_state_handler
        self.heading = Hx(0,0,0)
        self.focus = Hx(0,0,0)
        self.height = 2
        self.speed = DEFAULT_SPEED
        self.speed_ang_x = self.speed*math.cos(60*math.pi/180)
        self.speed_ang_y = ISO_SCALE*self.speed*math.sin(60*math.pi/180)

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

        self.px = Px(0,0,0)
        self.collider = collision.Poly(collision.Vector(0, 0), [collision.Vector(it.x, it.y) for it in self.px.vertices(7, ORIENTATION_FLAT)], 0)

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

    def on_action(self, evt, hx, *args):
        if(evt == "on_overlay"): self.dispatch_event(evt, self.hx+self.heading+hx, *args)
        elif(evt == "on_jump"): self.dispatch_event("on_try", "do_actor_move", self, self.focus.into_px())

    def update(self, dt):
        self.collider.pos = collision.Vector(self.px.x, self.px.y)
        if(self.px.into_hx() != self.hx): self.hx = self.px_into_hx()

        was_px = self.px

        now_px = copy.copy(self.px)
        if not(self.key_handler[key.LEFT] or self.key_handler[key.RIGHT] or self.key_handler[key.UP] or self.key_handler[key.DOWN]):
            if self.sprite.image == self.animations["walk_n"]: self.sprite.image = self.animations["stand_n"]
            if self.sprite.image == self.animations["walk_e"]: self.sprite.image = self.animations["stand_e"]
            if self.sprite.image == self.animations["walk_w"]: self.sprite.image = self.animations["stand_w"]
            if self.sprite.image == self.animations["walk_s"]: self.sprite.image = self.animations["stand_s"]

        if self.key_handler[key.UP]: 
            if self.sprite.image != self.animations["walk_n"]: 
                self.sprite.image = self.animations["walk_n"]
            if self.key_handler[key.LEFT] or not self.key_handler[key.RIGHT] and (self.heading == Hx(-1,0,0) or self.heading == Hx(-1,+1,0) or self.heading == Hx(+1,-1,0)):
                now_px.x -= self.speed_ang_x*dt
                now_px.y += self.speed_ang_y*dt
                self.heading = Hx(-1,+1,0)
            else:
                now_px.x += self.speed_ang_x*dt
                now_px.y += self.speed_ang_y*dt
                self.heading = Hx(0,+1,0)
        if self.key_handler[key.DOWN]: 
            if self.sprite.image != self.animations["walk_s"]:
                self.sprite.image = self.animations["walk_s"]
            if self.key_handler[key.RIGHT] or not self.key_handler[key.LEFT] and (self.heading == Hx(1,0,0) or self.heading == Hx(+1,-1,0) or self.heading == Hx(-1,1,0)):
                now_px.x += self.speed_ang_x*dt
                now_px.y -= self.speed_ang_y*dt
                self.heading = Hx(+1,-1,0)
            else:
                now_px.x -= self.speed_ang_x*dt
                now_px.y -= self.speed_ang_y*dt
                self.heading = Hx(0,-1,0)
        
        if self.key_handler[key.RIGHT] and not(self.key_handler[key.UP] or self.key_handler[key.DOWN]):
            now_px.x += self.speed*dt
            self.heading = Hx(+1,+0,0)
            if self.sprite.image != self.animations["walk_e"] and not(self.key_handler[key.UP] or self.key_handler[key.DOWN]): 
                self.sprite.image = self.animations["walk_e"]
        
        if self.key_handler[key.LEFT] and not(self.key_handler[key.UP] or self.key_handler[key.DOWN]):
            now_px.x -= self.speed*dt
            self.heading = Hx(-1,+0,0)
            if self.sprite.image != self.animations["walk_w"] and not(self.key_handler[key.UP] or self.key_handler[key.DOWN]): 
                self.sprite.image = self.animations["walk_w"]

        if(now_px != was_px): self.dispatch_event('on_try',ActorMoveEvent(self.id, (now_px.x,now_px.y,now_px.z), dt))

    def recalc(self):
        was_focus_hx = self.focus
        # was_position = self.sprite.position

        self.sprite.position = self._px.into_screen((0,0,1))
        self.focus = self.hx+self.heading

        now_focus_hx = self.hx+self.heading
        # now_position = self.sprite.position

        if now_focus_hx != was_focus_hx: self.dispatch_event('on_looking_at', self, now_focus_hx, was_focus_hx)

Actor.register_event_type('on_try')
Actor.register_event_type('on_looking_at')
Actor.register_event_type('on_overlay')
Actor.register_event_type('on_jump')

class Factory:
    def __init__(self, key_state_handler, batch):
        self.key_state_handler = key_state_handler
        self.batch = batch

    def create(self, id):
        return Actor(id, self.key_state_handler if id == 0 else None, self.batch)

class ImplFactory:
    def create(self, id):
        return Impl(id)        