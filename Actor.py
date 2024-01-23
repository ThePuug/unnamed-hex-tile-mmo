import math
import pyglet
from pyglet.window import key
from logging import debug
from Scene import Scene

from Tile import Hx, Px, Tile

SPEED = 90
SPEED_ANG_X = SPEED*math.cos(60*math.pi/180)
SPEED_ANG_Y = SPEED*math.sin(60*math.pi/180)

class Actor(pyglet.event.EventDispatcher):
    def __init__(self, key_state_handler, batch, groups):
        self.z = 0
        self.groups = groups
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
        self.sprite = pyglet.sprite.Sprite(self.animations["walk_s"], group=self.groups[self.z+2], batch=batch)
        self.sprite.scale = 0.66
        self.key_handler = key_state_handler
        self.heading = Hx(0,0,0)
        self.focus = pyglet.shapes.Polygon(*[[it.x,it.y] for it in Tile(self.px.z).into_polygon()],color=(255,255,150,50), batch=batch,group=self.groups[self.z+1])
        self.focus.anchor_position = (-Tile.WIDTH/2,-Tile.HEIGHT/2)

    @property
    def px(self): return Px(self.sprite.position[0],self.sprite.position[1],self.z)

    def on_key_press(self,sym,mod):
        if(sym == key.C): self.dispatch_event('on_build',Px(self.focus.x,self.focus.y,self.z).into_hx())

    def update(self, dt):
        if not(self.key_handler[key.LEFT] or self.key_handler[key.RIGHT] or self.key_handler[key.UP] or self.key_handler[key.DOWN]):
            if self.sprite.image == self.animations["walk_n"]: self.sprite.image = self.animations["stand_n"]
            if self.sprite.image == self.animations["walk_e"]: self.sprite.image = self.animations["stand_e"]
            if self.sprite.image == self.animations["walk_w"]: self.sprite.image = self.animations["stand_w"]
            if self.sprite.image == self.animations["walk_s"]: self.sprite.image = self.animations["stand_s"]

        if self.key_handler[key.UP]: 
            if self.sprite.image != self.animations["walk_n"]: 
                self.sprite.image = self.animations["walk_n"]
            if self.key_handler[key.LEFT]:
                self.sprite.x -= SPEED_ANG_X*dt
                self.sprite.y += SPEED_ANG_Y*dt
                self.heading = Hx(-1,+1,self.z)
            elif self.key_handler[key.RIGHT]:
                self.sprite.x += SPEED_ANG_X*dt
                self.sprite.y += SPEED_ANG_Y*dt
                self.heading = Hx(0,+1,self.z)
            else:
                self.sprite.y += SPEED*dt
                if(self.heading == Hx(-1,0,self.z) or self.heading == Hx(-1,+1,self.z) or self.heading == Hx(0,-1,self.z)): self.heading = Hx(-1,+1,self.z)
                else: self.heading = Hx(0,+1,self.z)
        if self.key_handler[key.DOWN]: 
            if self.sprite.image != self.animations["walk_s"]:
                self.sprite.image = self.animations["walk_s"]
            if self.key_handler[key.LEFT]:
                self.sprite.x -= SPEED_ANG_X*dt
                self.sprite.y -= SPEED_ANG_Y*dt
                self.heading = Hx(0,-1,self.z)
            elif self.key_handler[key.RIGHT]:
                self.sprite.x += SPEED_ANG_X*dt
                self.sprite.y -= SPEED_ANG_Y*dt
                self.heading = Hx(+1,-1,self.z)
            else:
                self.sprite.y -= SPEED*dt
                if(self.heading == Hx(1,0,self.z) or self.heading == Hx(+1,-1,self.z) or self.heading == Hx(0,1,self.z)): self.heading = Hx(+1,-1,self.z)
                else: self.heading = Hx(0,-1,self.z)
        
        if self.key_handler[key.RIGHT] and not(self.key_handler[key.UP] or self.key_handler[key.DOWN]):
            self.sprite.x += SPEED*dt
            self.heading = Hx(+1,+0,self.z)
            if self.sprite.image != self.animations["walk_e"] and not(self.key_handler[key.UP] or self.key_handler[key.DOWN]): 
                self.sprite.image = self.animations["walk_e"]
        
        if self.key_handler[key.LEFT] and not(self.key_handler[key.UP] or self.key_handler[key.DOWN]):
            self.sprite.x -= SPEED*dt
            self.heading = Hx(-1,+0,self.z)
            if self.sprite.image != self.animations["walk_w"] and not(self.key_handler[key.UP] or self.key_handler[key.DOWN]): 
                self.sprite.image = self.animations["walk_w"]  

        focus = Px(self.focus.position[0],self.focus.position[1],self.z).into_hx()
        curr_hx = Px(self.sprite.x,self.sprite.y,self.z).into_hx()
        new_focus_hx = Hx(self.heading.q+curr_hx.q,self.heading.r+curr_hx.r,self.z)
        if new_focus_hx.q != focus.q or new_focus_hx.r != focus.r:
            self.dispatch_event('on_looking_at',new_focus_hx)
            new_focus_px = new_focus_hx.into_px()
            self.focus.position = (new_focus_px.x,new_focus_px.y)

Actor.register_event_type('on_looking_at')
Actor.register_event_type('on_build')
