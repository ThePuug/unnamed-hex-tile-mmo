
from math import cos, floor, pi, sin, sqrt
import math

ORIENTATION = [[sqrt(3), sqrt(3) / 2, 0, 3.0/2], 
               [sqrt(3) / 3, -1 / 3, 0, 2.0/3, 0.5]]

class Px:
    def __init__(self, x:int, y:int):
        self._x = x
        self._y = y

    def __hash__(self): return hash((self._x,self._y))
    def __eq__(self,other): return self._x==other.x and self._y==other.y

    @property
    def x(self): return self._x
    @x.setter
    def x(self,v): self._x = v

    @property
    def y(self): return self._y
    @y.setter
    def y(self,v): self._y = v

    def into_hx(self, size: int):
        pt = Px(self._x / size, self._y / size)
        q = ORIENTATION[1][0] * pt.x + ORIENTATION[1][1] * pt.y
        r = ORIENTATION[1][2] * pt.x + ORIENTATION[1][3] * pt.y
        return self.hex_round(q,r)
    
    def hex_round(self, aq, ar):
        q = int(round(aq))
        r = int(round(ar))
        s = int(round(-aq-ar))
        q_diff = abs(q - aq)
        r_diff = abs(r - ar)
        s_diff = abs(s - (-aq-ar))
        if (q_diff > r_diff and q_diff > s_diff):
            q = -r-s
        elif (r_diff > s_diff):
            r = -q-s
        else:
            s = -q-r
        return Hx(q, r)

class Hx:
    def __init__(self, q:int, r: int):
        self._q = q
        self._r = r
    
    def __hash__(self): return hash((self._q,self._r))
    def __eq__(self,other): return self._q==other.q and self._r==other.r

    @property
    def q(self): return self._q
    @q.setter
    def q(self,v): self._q = v

    @property
    def r(self): return self._r
    @r.setter
    def r(self,v): self._r = v

    @property
    def s(self): return -self.q-self.r

    def into_px(self, size) -> Px:
        x = (ORIENTATION[0][0] * self._q + ORIENTATION[0][1] * self._r) * size
        y = (ORIENTATION[0][2] * self._q + ORIENTATION[0][3] * self._r) * size
        return Px(x,y - size/4)

class Tile:
    def __init__(self, size: int):
        self._size = size

    def hx_offset(self, corner: int) -> Px:
        angle = 2 * pi * (0.5+corner) / 6
        return Px(self._size * cos(angle), self._size * sin(angle))

    def into_polygon(self):
        corners = []
        for i in range(6):
            offset = self.hx_offset(i)
            corners.append(Px(offset.x, offset.y))
        return corners