import pyglet
from logging import debug

class Actor(pyglet.event.EventDispatcher):
    def __init__(self, sprites):
        self._animations = sprites
        self._active_sprite = self._animations["walk_s"]

    def on_key_press(self,sym,mod):
        debug("args({},{})".format(sym,mod))
        if(sym == pyglet.window.key.W): self.active_sprite = self.sprites["walk_n"]
        elif(sym == pyglet.window.key.D): self.active_sprite = self.sprites["walk_e"]
        elif(sym == pyglet.window.key.S): self.active_sprite = self.sprites["walk_s"]
        elif(sym == pyglet.window.key.A): self.active_sprite = self.sprites["walk_w"]

    def draw(self): 
        self._active_sprite.draw()

Actor.register_event_type('on_key_press')
