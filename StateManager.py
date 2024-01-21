from logging import debug
import pyglet
from pyglet.window import key

class StateManager:
    CONSOLE = 'console'
    ACTOR = 'actor'
    SCENE = 'scene'
    OVERLAY = 'overlay'

    STATE_PLAY          = 1 << 0
    STATE_UI_CONSOLE    = 1 << 1
    STATE_UI_BUILD      = 1 << 2

    def __init__(self, window, key_state_handler):
        self.window = window
        self.key_state_handler = key_state_handler
        self.state = 0
        self.them = {}

    def on_key_press(self,sym,mod):
        if(self.state & StateManager.STATE_PLAY):
            if(sym == key.ESCAPE):
                self.window.pop_handlers()
                self.window.push_handlers(self.key_state_handler)
                self.window.push_handlers(self.them[StateManager.ACTOR])
                self.state = StateManager.STATE_PLAY if self.state != StateManager.STATE_PLAY else 0
            elif(self.state == StateManager.STATE_PLAY and sym == key.QUOTELEFT):
                self.window.pop_handlers()
                self.window.pop_handlers()
                self.window.push_handlers(self.them[StateManager.CONSOLE])
                self.them[StateManager.CONSOLE].toggle()
                self.state |= StateManager.STATE_UI_CONSOLE
            elif(self.state == StateManager.STATE_PLAY and sym == key.C):
                self.window.pop_handlers()
                self.window.pop_handlers()
                self.window.push_handlers(self.them[StateManager.OVERLAY])
                self.state |= StateManager.STATE_UI_BUILD
            elif(self.state & StateManager.STATE_UI_CONSOLE and sym == key.ENTER
                 or self.state & StateManager.STATE_UI_BUILD and sym == key.SPACE
                 or self.state & StateManager.STATE_UI_BUILD and sym == key.B):
                self.window.pop_handlers()
                self.window.push_handlers(self.key_state_handler)
                self.window.push_handlers(self.them[StateManager.ACTOR])
                self.state = StateManager.STATE_PLAY
        if(self.state != 0): return pyglet.event.EVENT_HANDLED

    def begin(self):
        self.window.push_handlers(self)
        self.window.push_handlers(self.key_state_handler)
        self.window.push_handlers(self.them[StateManager.ACTOR])
        self.them[StateManager.ACTOR].push_handlers(self.them[StateManager.SCENE])
        self.them[StateManager.ACTOR].push_handlers(self.them[StateManager.OVERLAY])
        self.them[StateManager.SCENE].push_handlers(self.them[StateManager.ACTOR])
        self.them[StateManager.OVERLAY].push_handlers(self.them[StateManager.SCENE])
        self.state |= StateManager.STATE_PLAY

    def register(self,id,it):
        self.them[id] = it
