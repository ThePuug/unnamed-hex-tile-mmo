import quickle

class Event(quickle.Struct):
    pass

class ActorMoveEvent(Event):
    actor: quickle.Struct
    dt: float
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
    data: bytes
    event = "load_scene"

class TileChangeEvent(Event):
    hx: tuple
    typ: str
    idx: int
    event = "change_tile"

class TileDiscoverEvent(Event):
    hx: tuple
    event = "discover_tile"

REGISTRY = [ActorMoveEvent, 
            ActorLoadEvent, 
            ActorUnloadEvent, 
            ConnectionInitEvent, 
            SceneLoadEvent,
            TileChangeEvent,
            TileDiscoverEvent]
