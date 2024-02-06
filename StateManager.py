from logging import debug
import pyglet
from Event import ActorLoadEvent

from Session import Session

SCENE = 'scene'
OVERLAY = 'overlay'
ACTION_BAR = 'action_bar'

STATE_PLAY       = 1 << 0
STATE_UI_OVERLAY = 1 << 1

class Impl(pyglet.event.EventDispatcher):
    def __init__(self,actor_factory):
        self.actor_factory = actor_factory
        self.state = 0
        self.them = {}

    def do_load_actor(self, tid, evt):
        id = tid if evt.is_self else evt.id
        actor = self.actor_factory.create(id)
        self.push_handlers(actor)
        actor.push_handlers(self)
        # self.them[SCENE].push_handlers(actor)
        self.them[SCENE].actors[id] = actor

    def begin(self):
        self.push_handlers(self.them[SCENE])
        self.state |= STATE_PLAY

    def register(self,id,it):
        self.them[id] = it

Impl.register_event_type('do_move_actor')
Impl.register_event_type('on_try')
Impl.register_event_type('on_confirm')

class StateManager(Impl):
    def __init__(self, session, window, key_state_handler, actor_factory):
        super().__init__(actor_factory)
        self.session = session
        self.window = window
        self.key_state_handler = key_state_handler

    def on_try(self, e, sync=False):
        self.session.send(e)
        if not sync: self.dispatch_event(e.event, 0, e)

    def do_load_actor(self, evt):
        super().do_load_actor(0, evt)
        if evt.is_self: self.them[ACTION_BAR].push_handlers(self.them[SCENE].actors[evt.id])

    def on_overlay(self,*args):
        if(self.state & STATE_PLAY):
            self.window.pop_handlers()
            self.window.pop_handlers()
            self.window.push_handlers(self.them[OVERLAY])
            self.state |= STATE_UI_OVERLAY
            self.dispatch_event("on_open",*args)

    def on_close(self,*args):
        if(self.state & STATE_UI_OVERLAY):
            self.window.pop_handlers()
            self.window.push_handlers(self.key_state_handler)
            self.window.push_handlers(self.them[ACTION_BAR])
            self.state = STATE_PLAY
        else:
            self.them[SCENE].to_file()

    def begin(self):
        super().begin()
        self.dispatch_event('do_load_actor',ActorLoadEvent(0, True))
        self.session.push_handlers(self)
        self.window.push_handlers(self)
        self.window.push_handlers(self.key_state_handler)
        self.window.push_handlers(self.them[ACTION_BAR])
        self.push_handlers(self.them[OVERLAY])
        self.them[OVERLAY].push_handlers(self)
        self.them[OVERLAY].push_handlers(self.them[SCENE])

    def update(self, dt):
        started = pyglet.clock._time.time()
        processed = 0
        for tid, seq, evt in self.session.recv():
            self.dispatch_event(evt.event, tid, evt)
            processed += 1
        if processed > 0: debug("processed {} in {}".format(processed, pyglet.clock._time.time()-started))
    
    def confirm(self, tid, seq, evt): pass


StateManager.register_event_type('on_open')
StateManager.register_event_type('do_move_to')
StateManager.register_event_type('do_move_actor')
StateManager.register_event_type('do_load_actor')