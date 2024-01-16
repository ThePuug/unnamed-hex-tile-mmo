from logging import debug
import pyglet

class StateManager(pyglet.event.EventDispatcher):
    KEY_STATE = 'key_state'
    CONSOLE = 'console'
    WINDOW = 'window'
    ACTOR = 'actor'
    SCENE = 'scene'
    UI = 'ui'

    STATE_PLAY          = 1 << 0
    STATE_UI_CONSOLE    = 1 << 1

    def __init__(self):
        self.state = None
        self.them = {}

    def on_key_press(self,sym,mod):
        curr = self.state
        debug('args({},{})'.format(sym,mod))
        if(curr & StateManager.STATE_PLAY):
            if(sym == pyglet.window.key.QUOTELEFT):
                self.state |= StateManager.STATE_UI_CONSOLE
        if(curr & StateManager.STATE_UI_CONSOLE):
            if(sym == pyglet.window.key.ESCAPE): 
                self.state &= ~StateManager.STATE_UI_CONSOLE

        turnoff = curr ^ self.state & ~self.state
        if(turnoff & StateManager.STATE_UI_CONSOLE):
            self.push_handlers()


    def begin(self,window):
        window.push_handlers(self,self.them[StateManager.KEY_STATE],self.them[StateManager.ACTOR])
        self.state = self.STATE_PLAY

    def register(self,id,it):
        self.them[id] = it
