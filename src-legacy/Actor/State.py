from dataclasses import dataclass

@dataclass
class State:
    id: int
    height: int
    heading: tuple
    speed: int
    last_clock: float
    vertical: float
    air_dz: float
    air_time: float
    px: tuple
    typ: str
    busy: False
