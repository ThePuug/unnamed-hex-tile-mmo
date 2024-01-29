import copy
import math
import pyglet
from pyglet.window import key

from HxPx import Hx, Px

DEFAULT_SPEED = 90

class Actor(pyglet.event.EventDispatcher):
    def __init__(self, key_state_handler, batch, groups):
        self.key_handler = key_state_handler
        self.groups = groups
        self.at = Hx(0,0,1)
        self.heading = copy.copy(self.at)
        self.focus = pyglet.shapes.Polygon(*[[it.x,it.y] for it in self.at.vertices],color=(255,255,150,50), batch=batch, group=self.groups[self.at.z+1])
        self.focus.anchor_position = (-self.at.width/2,-self.at.height/2)
        self.speed = DEFAULT_SPEED
        self.speed_ang_y = self.speed*math.cos(60*math.pi/180)
        self.speed_ang_x = self.at.iso_scale*self.speed*math.sin(60*math.pi/180)

        frames_blank = pyglet.image.ImageGrid(pyglet.resource.image("assets/sprites/blank.png"),rows=4,columns=4)
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
        self.sprite = pyglet.sprite.Sprite(self.animations["walk_s"], group=self.groups[self.at.z+1], batch=batch)
        self.sprite.scale = 0.66

    @property
    def px(self): return Px(self.sprite.position,self.at.z)

    def on_action(self,evt,*args):
        self.dispatch_event(evt,Px(self.focus.position,self.at.z-1).into_hx(),*args)

    def update(self, dt):
        was = self.px
        if not(self.key_handler[key.LEFT] or self.key_handler[key.RIGHT] or self.key_handler[key.UP] or self.key_handler[key.DOWN]):
            if self.sprite.image == self.animations["walk_n"]: self.sprite.image = self.animations["stand_n"]
            if self.sprite.image == self.animations["walk_e"]: self.sprite.image = self.animations["stand_e"]
            if self.sprite.image == self.animations["walk_w"]: self.sprite.image = self.animations["stand_w"]
            if self.sprite.image == self.animations["walk_s"]: self.sprite.image = self.animations["stand_s"]

        if self.key_handler[key.UP]: 
            if self.sprite.image != self.animations["walk_n"]: 
                self.sprite.image = self.animations["walk_n"]
            if self.key_handler[key.LEFT]:
                self.sprite.x -= self.speed_ang_x*dt
                self.sprite.y += self.speed_ang_y*dt
                self.heading = Hx(-1,+1,self.at.z)
            elif self.key_handler[key.RIGHT]:
                self.sprite.x += self.speed_ang_x*dt
                self.sprite.y += self.speed_ang_y*dt
                self.heading = Hx(0,+1,self.at.z)
            else:
                self.sprite.y += self.speed*dt
                if(self.heading == Hx(-1,0,self.at.z) or self.heading == Hx(-1,+1,self.at.z) or self.heading == Hx(0,-1,self.at.z)): self.heading = Hx(-1,+1,self.at.z)
                else: self.heading = Hx(0,+1,self.at.z)
        if self.key_handler[key.DOWN]: 
            if self.sprite.image != self.animations["walk_s"]:
                self.sprite.image = self.animations["walk_s"]
            if self.key_handler[key.LEFT]:
                self.sprite.x -= self.speed_ang_x*dt
                self.sprite.y -= self.speed_ang_y*dt
                self.heading = Hx(0,-1,self.at.z)
            elif self.key_handler[key.RIGHT]:
                self.sprite.x += self.speed_ang_x*dt
                self.sprite.y -= self.speed_ang_y*dt
                self.heading = Hx(+1,-1,self.at.z)
            else:
                self.sprite.y -= self.speed*dt
                if(self.heading == Hx(1,0,self.at.z) or self.heading == Hx(+1,-1,self.at.z) or self.heading == Hx(0,1,self.at.z)): self.heading = Hx(+1,-1,self.at.z)
                else: self.heading = Hx(0,-1,self.at.z)
        
        if self.key_handler[key.RIGHT] and not(self.key_handler[key.UP] or self.key_handler[key.DOWN]):
            self.sprite.x += self.speed*dt
            self.heading = Hx(+1,+0,self.at.z)
            if self.sprite.image != self.animations["walk_e"] and not(self.key_handler[key.UP] or self.key_handler[key.DOWN]): 
                self.sprite.image = self.animations["walk_e"]
        
        if self.key_handler[key.LEFT] and not(self.key_handler[key.UP] or self.key_handler[key.DOWN]):
            self.sprite.x -= self.speed*dt
            self.heading = Hx(-1,+0,self.at.z)
            if self.sprite.image != self.animations["walk_w"] and not(self.key_handler[key.UP] or self.key_handler[key.DOWN]): 
                self.sprite.image = self.animations["walk_w"]

        now = Px(self.sprite.position,self.at.z)
        if(now != was): self.dispatch_event('on_move_to',self,now)

        now_hx = now.into_hx()
        was_focus_hx = Px(self.focus.position,self.at.z-1).into_hx()
        now_focus_hx = Hx(self.heading.q+now_hx.q,self.heading.r+now_hx.r,self.at.z-1)
        if now_focus_hx.q != was_focus_hx.q or now_focus_hx.r != was_focus_hx.r:
            now_focus_px = now_focus_hx.into_px()
            self.focus.position = (now_focus_px.x,now_focus_px.y)
            self.dispatch_event('on_looking_at',now_focus_hx)

Actor.register_event_type('on_looking_at')
Actor.register_event_type('on_move_to')
Actor.register_event_type('on_overlay')
