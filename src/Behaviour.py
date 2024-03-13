import random

from Event import ActorMoveEvent
from HxPx import Hx

class Behaviour:
    def update(self, actor): pass

class Dog(Behaviour):
    def update(self, actor, dt):
        if actor.busy > 0:
            actor.dispatch_event('on_try', actor.id, ActorMoveEvent(actor.state, dt), False)

        if actor.busy > -2:
            actor.busy -= dt
        else:
            actor.heading = random.choice([Hx(1,0,0), Hx(-1,0,0), 
                                        Hx(0,1,0), Hx(0,-1,0),
                                        Hx(1,-1,0), Hx(-1,1,0)])
            magnitude = random.randint(1,3)
            actor.busy = magnitude*.5
            actor.dispatch_event('on_try', actor.id, ActorMoveEvent(actor.state, dt), False)

class Factory:
    def create(self, typ):
        if typ == "dog": return Dog()