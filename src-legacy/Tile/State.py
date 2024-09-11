from dataclasses import dataclass

@dataclass
class State:
    flags: int
    typ: str
    idx: int
