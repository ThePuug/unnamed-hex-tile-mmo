from logging import debug
import pyglet

from Config import *
from Event import *

SCENE = 'scene'
OVERLAY = 'overlay'
ACTION_BAR = 'action_bar'

STATE_PLAY       = 1 << 0
STATE_UI_OVERLAY = 1 << 1

class Impl(pyglet.event.EventDispatcher):
    def __init__(self, session):
        self.session = session
        self.state = 0
        self.registry = {}

    # Impl tries everything, and sends back what is done
    def on_try(self, tid, evt): self.dispatch_event("try_{}".format(evt.event), tid, evt)
    def on_do(self, tid, evt):
        self.dispatch_event("do_{}".format(evt.event), tid, evt)
        self.session.send(evt, tid)

    def try_init_connection(self, tid, evt):
        evt.tid = tid
        for i,it in self.registry[SCENE].actors.items(): self.dispatch_event("on_do", tid, ActorLoadEvent(i,(it.px.x,it.px.y,it.px.z)))
        self.dispatch_event("on_do", tid, evt)

    def begin(self):
        self.push_handlers(self.registry[SCENE])
        self.registry[SCENE].push_handlers(self)
        self.state |= STATE_PLAY

    def update(self, dt):
        for tid, evt in self.session.recv():
            self.dispatch_event("on_try", tid, evt)

    def register(self, id, it):
        self.registry[id] = it

Impl.register_event_type('on_try')
Impl.register_event_type('on_do')
Impl.register_event_type('try_move_actor')
Impl.register_event_type('do_move_actor')
Impl.register_event_type('try_load_actor')
Impl.register_event_type('do_load_actor')
Impl.register_event_type('try_init_connection')
Impl.register_event_type('do_init_connection')

class StateManager(Impl):
    def __init__(self, session, window, key_state_handler):
        super().__init__(session)
        self.window = window
        self.key_state_handler = key_state_handler
        self.tid = None

    # Client sends everything it tries to server, and does everything it is told to
    def on_do(self, tid, evt): self.dispatch_event("do_{}".format(evt.event), tid, evt)
    def on_try(self, tid, evt, sync=False):
        if not sync: self.dispatch_event("try_{}".format(evt.event), tid, evt)
        self.session.send(evt, tid)
    
    def do_init_connection(self, tid, evt):
        self.tid = evt.tid
        self.dispatch_event('on_try', tid, ActorLoadEvent(evt.tid, (0,0,0)), True)

    def on_overlay(self, *args):
        if(self.state & STATE_PLAY):
            self.window.pop_handlers()
            self.window.pop_handlers()
            self.window.push_handlers(self.registry[OVERLAY])
            self.state |= STATE_UI_OVERLAY
            self.dispatch_event("on_open",*args)

    def on_close(self, *args):
        if(self.state & STATE_UI_OVERLAY):
            self.window.pop_handlers()
            self.window.push_handlers(self.key_state_handler)
            self.window.push_handlers(self.registry[ACTION_BAR])
            self.state = STATE_PLAY
        else:
            self.registry[SCENE].to_file()

    def begin(self):
        super().begin()
        self.window.push_handlers(self)
        self.window.push_handlers(self.key_state_handler)
        self.window.push_handlers(self.registry[ACTION_BAR])
        self.push_handlers(self.registry[OVERLAY])
        self.registry[OVERLAY].push_handlers(self)
        self.registry[OVERLAY].push_handlers(self.registry[SCENE])    
        self.dispatch_event('on_try', None, ConnectionInitEvent(None), True)

    def update(self, dt):
        for tid, evt in self.session.recv():
            self.dispatch_event("do_{}".format(evt.event), tid, evt)

StateManager.register_event_type('on_open')
