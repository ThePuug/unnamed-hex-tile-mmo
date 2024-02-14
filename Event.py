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

class OverlaySelectEvent(Event):
    hx: tuple
    typ: str
    idx: int
    event = "select_overlay"

class SceneLoadEvent(Event):
    data: bytes
    event = "load_scene"

REGISTRY = [ActorMoveEvent, 
            ActorLoadEvent, 
            ActorUnloadEvent, 
            ConnectionInitEvent, 
            OverlaySelectEvent,
            SceneLoadEvent]
