from dataclasses import dataclass

from Actor.State import State as Actor
from Tile.State import State as Tile

class Event:
    def from_dict(d):
        k, v = list(d.items())[0]
        for t in Event.__subclasses__():
            if k == t.__name__:
                return t(*v)
            
@dataclass
class ActorMove(Event):
    actor: Actor
    dt: float

@dataclass
class ActorLoad(Event):
    actor: Actor

@dataclass
class ActorUnload(Event):
    actor: Actor

@dataclass
class ConnectionInit(Event):
    tid: int

@dataclass
class SceneLoad(Event):
    pass

@dataclass
class TileChange(Event):
    hx: tuple
    tile: Tile

@dataclass
class TileDiscover(Event):
    hx: tuple
