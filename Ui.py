from logging import debug
import pyglet
from pyglet.window import key

class Ui(pyglet.event.EventDispatcher):
    def __init__(self, console):
        self.console = console

    def on_key_press(self,sym,mod):
        if(sym == key.E):
            debug('E!')

Ui.register_event_type('on_key_press')