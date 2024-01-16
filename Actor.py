import math
import pyglet
from pyglet.window import key
from logging import debug

from Tile import Px

SPEED = 90
SPEED_ANG_X = SPEED*math.cos(60*math.pi/180) #30
SPEED_ANG_Y = SPEED*math.sin(60*math.pi/180) #52

class Actor(pyglet.event.EventDispatcher):
    def __init__(self, sprite, animations, key_state_handler):
        self._sprite = sprite
        self._animations = animations
        self._key_handler = key_state_handler

    @property
    def px(self): return Px(self._sprite.position[0],self._sprite.position[1])

    def update(self, dt):
        if not(self._key_handler[key.LEFT] or self._key_handler[key.RIGHT] or self._key_handler[key.UP] or self._key_handler[key.DOWN]):
            if self._sprite.image == self._animations["walk_n"]: self._sprite.image = self._animations["stand_n"]
            if self._sprite.image == self._animations["walk_e"]: self._sprite.image = self._animations["stand_e"]
            if self._sprite.image == self._animations["walk_w"]: self._sprite.image = self._animations["stand_w"]
            if self._sprite.image == self._animations["walk_s"]: self._sprite.image = self._animations["stand_s"]

        if self._key_handler[key.UP]: 
            if self._sprite.image != self._animations["walk_n"]: 
                self._sprite.image = self._animations["walk_n"]
            if self._key_handler[key.LEFT]:
                self._sprite.x -= SPEED_ANG_X*dt
                self._sprite.y += SPEED_ANG_Y*dt
            elif self._key_handler[key.RIGHT]:
                self._sprite.x += SPEED_ANG_X*dt
                self._sprite.y += SPEED_ANG_Y*dt
            else:
                self._sprite.y += SPEED*dt
        if self._key_handler[key.DOWN]: 
            if self._sprite.image != self._animations["walk_s"]:
                self._sprite.image = self._animations["walk_s"]
            if self._key_handler[key.LEFT]:
                self._sprite.x -= SPEED_ANG_X*dt
                self._sprite.y -= SPEED_ANG_Y*dt
            elif self._key_handler[key.RIGHT]:
                self._sprite.x += SPEED_ANG_X*dt
                self._sprite.y -= SPEED_ANG_Y*dt
            else:
                self._sprite.y -= SPEED*dt
        
        if self._key_handler[key.RIGHT] and not(self._key_handler[key.UP] or self._key_handler[key.DOWN]):
            self._sprite.x += SPEED*dt
            if self._sprite.image != self._animations["walk_e"] and not(self._key_handler[key.UP] or self._key_handler[key.DOWN]): 
                self._sprite.image = self._animations["walk_e"]
        
        if self._key_handler[key.LEFT] and not(self._key_handler[key.UP] or self._key_handler[key.DOWN]):
            self._sprite.x -= SPEED*dt
            if self._sprite.image != self._animations["walk_w"] and not(self._key_handler[key.UP] or self._key_handler[key.DOWN]): 
                self._sprite.image = self._animations["walk_w"]
