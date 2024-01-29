import pyglet

class StateManager(pyglet.event.EventDispatcher):
    CONSOLE = 'console'
    ACTOR = 'actor'
    SCENE = 'scene'
    OVERLAY = 'overlay'
    ACTION_BAR = 'action_bar'

    STATE_PLAY          = 1 << 0
    STATE_UI_CONSOLE    = 1 << 1
    STATE_UI_OVERLAY    = 1 << 2

    def __init__(self, window, key_state_handler):
        self.window = window
        self.key_state_handler = key_state_handler
        self.state = 0
        self.them = {}

    def on_overlay(self,*args):
        if(self.state & StateManager.STATE_PLAY):
            self.window.pop_handlers()
            self.window.pop_handlers()
            self.window.push_handlers(self.them[StateManager.OVERLAY])
            self.state |= StateManager.STATE_UI_OVERLAY
            self.dispatch_event("on_open",*args)

    def on_close(self,*args):
        if(self.state & StateManager.STATE_UI_OVERLAY):
            self.window.pop_handlers()
            self.window.push_handlers(self.key_state_handler)
            self.window.push_handlers(self.them[StateManager.ACTION_BAR])
            self.state = StateManager.STATE_PLAY

    def begin(self):
        self.window.push_handlers(self)
        self.window.push_handlers(self.key_state_handler)
        self.window.push_handlers(self.them[StateManager.ACTION_BAR])
        self.push_handlers(self.them[StateManager.OVERLAY])
        self.them[StateManager.ACTOR].push_handlers(self)
        self.them[StateManager.OVERLAY].push_handlers(self)
        self.them[StateManager.ACTOR].push_handlers(self.them[StateManager.SCENE])
        self.them[StateManager.ACTION_BAR].push_handlers(self.them[StateManager.ACTOR])
        self.them[StateManager.OVERLAY].push_handlers(self.them[StateManager.SCENE])
        self.them[StateManager.SCENE].push_handlers(self.them[StateManager.ACTOR])
        self.state |= StateManager.STATE_PLAY

    def register(self,id,it):
        self.them[id] = it

StateManager.register_event_type('on_open')