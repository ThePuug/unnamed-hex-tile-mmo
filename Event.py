import quickle

class Event(quickle.Struct):
    pass

class ActorMoveEvent(Event):
    id: int
    heading: tuple
    dt: float
    pos: tuple
    event = "move_actor"

class ActorLoadEvent(Event):
    id: int
    pos: tuple
    event = "load_actor"

class ActorUnloadEvent(Event):
    id: int
    event = "unload_actor"

class ConnectionInitEvent(Event):
    tid: int
    event = "init_connection"

class SceneLoadEvent(Event):
    filename: str
    md5: str
    event = "load_scene"

REGISTRY = [ActorMoveEvent, ActorLoadEvent, ActorUnloadEvent, ConnectionInitEvent]
