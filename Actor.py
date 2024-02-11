import collision
import pyglet
from pyglet.window import key

from Config import *
from Event import ActorMoveEvent
from HxPx import Hx, Px
from Asset import DepthSprite, depth_shader

DEFAULT_SPEED = 90

class Impl(pyglet.event.EventDispatcher):
    def __init__(self, evt):
        self.id = evt.id
        self.last_clock = 0
        self.heading = Hx(0,0,0)
        self.focus = Hx(0,0,0)
        self.height = 2
        self.speed = DEFAULT_SPEED
        self.px = Px(*(evt.pos if evt.pos is not None else (0,0,0)))
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

    def update(self, dt):
        self.collider.pos = collision.Vector(self.px.x, self.px.y)
        if(self.px.into_hx() != self.hx): self.hx = self.px_into_hx() # TODO: remove this hack and do jumping right

    def recalc(self):
        was_focus_hx = self.focus
        self.focus = self.hx+self.heading
        now_focus_hx = self.hx+self.heading
        if now_focus_hx != was_focus_hx: self.dispatch_event('on_looking_at', self, now_focus_hx, was_focus_hx)

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
        elif(evt == "on_jump"): self.dispatch_event("on_try", "do_actor_move", self, self.focus.into_px())

    def update(self, dt):
        super().update(dt)
        if not(self.key_state[key.LEFT] or self.key_state[key.RIGHT] or self.key_state[key.UP] or self.key_state[key.DOWN]):
            if self.sprite.image == self.animations["walk_n"]: self.sprite.image = self.animations["stand_n"]
            if self.sprite.image == self.animations["walk_e"]: self.sprite.image = self.animations["stand_e"]
            if self.sprite.image == self.animations["walk_w"]: self.sprite.image = self.animations["stand_w"]
            if self.sprite.image == self.animations["walk_s"]: self.sprite.image = self.animations["stand_s"]
        else:
            if self.key_state[key.UP]: 
                if self.sprite.image != self.animations["walk_n"]: 
                    self.sprite.image = self.animations["walk_n"]
                if self.key_state[key.LEFT] or not self.key_state[key.RIGHT] and (self.heading == Hx(-1,0,0) or self.heading == Hx(-1,+1,0) or self.heading == Hx(+1,-1,0)):
                    self.heading = Hx(-1,+1,0)
                else:
                    self.heading = Hx(0,+1,0)
            if self.key_state[key.DOWN]: 
                if self.sprite.image != self.animations["walk_s"]:
                    self.sprite.image = self.animations["walk_s"]
                if self.key_state[key.RIGHT] or not self.key_state[key.LEFT] and (self.heading == Hx(1,0,0) or self.heading == Hx(+1,-1,0) or self.heading == Hx(-1,1,0)):
                    self.heading = Hx(+1,-1,0)
                else:
                    self.heading = Hx(0,-1,0)
            
            if self.key_state[key.RIGHT] and not(self.key_state[key.UP] or self.key_state[key.DOWN]):
                self.heading = Hx(+1,+0,0)
                if self.sprite.image != self.animations["walk_e"] and not(self.key_state[key.UP] or self.key_state[key.DOWN]): 
                    self.sprite.image = self.animations["walk_e"]
            
            if self.key_state[key.LEFT] and not(self.key_state[key.UP] or self.key_state[key.DOWN]):
                self.heading = Hx(-1,+0,0)
                if self.sprite.image != self.animations["walk_w"] and not(self.key_state[key.UP] or self.key_state[key.DOWN]): 
                    self.sprite.image = self.animations["walk_w"]

            self.dispatch_event('on_try', self.id, ActorMoveEvent(self.id, (self.heading.q, self.heading.r, self.heading.z), dt, None))

    def recalc(self):
        super().recalc()
        # was_position = self.sprite.position
        # now_position = self.sprite.position
        self.sprite.position = self._px.into_screen((0,0,1))

Actor.register_event_type('on_overlay')
Actor.register_event_type('on_jump')

class Factory:
    def __init__(self, key_state_handler, batch):
        self.key_state_handler = key_state_handler
        self.batch = batch

    def create(self, evt):
        return Actor(evt, self.key_state_handler, self.batch)

class ImplFactory:
    def create(self, evt):
        return Impl(evt)        