import quickle

class Event(quickle.Struct):
    pass

class ActorMoveEvent(Event):
    id: int
    pos: tuple
    dt: float
    event = "do_move_actor"

class ActorLoadEvent(Event):
    id: int
    is_self: bool
    event = "do_load_actor"