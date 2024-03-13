import quickle

class Event(quickle.Struct):
    pass

class ActorMoveEvent(Event):
    actor: quickle.Struct
    dt: float
    event = "move_actor"

class ActorLoadEvent(Event):
    actor: quickle.Struct
    event = "load_actor"

class ActorUnloadEvent(Event):
    actor: quickle.Struct
    event = "unload_actor"

class ConnectionInitEvent(Event):
    tid: int
    event = "init_connection"

class SceneLoadEvent(Event):
    event = "load_scene"

class TileChangeEvent(Event):
    hx: quickle.Struct
    tile: quickle.Struct
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
