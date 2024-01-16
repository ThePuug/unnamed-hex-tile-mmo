from logging import debug
import pyglet

class StateManager(pyglet.window.EventDispatcher):
    CONSOLE = 'console'
    ACTOR = 'actor'
    SCENE = 'scene'
    UI = 'ui'

    STATE_PLAY          = 1 << 0
    STATE_UI_CONSOLE    = 1 << 1

    def __init__(self, window, key_state_manager):
        self.window = window
        self.key_state_manager = key_state_manager
        self.state = None
        self.them = {}

    def on_key_press(self,sym,mod):
        curr = self.state
        debug('args({},{})'.format(sym,mod))
        if(curr & StateManager.STATE_PLAY):
            if(sym == pyglet.window.key.QUOTELEFT and self.state & ~StateManager.STATE_UI_CONSOLE):
                self.window.push_handlers(self.them[StateManager.CONSOLE])
                self.state &= ~StateManager.STATE_PLAY
                self.state |= StateManager.STATE_UI_CONSOLE
        if(curr & StateManager.STATE_UI_CONSOLE):
            if(sym == pyglet.window.key.ESCAPE): 
                self.window.pop_handlers(self.them[StateManager.CONSOLE])
                self.state &= ~StateManager.STATE_UI_CONSOLE

    def begin(self):
        self.window.push_handlers(self, self.key_state_manager, self.them[StateManager.ACTOR])
        self.state = self.STATE_PLAY

    def register(self,id,it):
        self.them[id] = it