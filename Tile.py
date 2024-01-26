from math import cos, pi, sin, sqrt
import math

ORIENTATION = [[sqrt(3), sqrt(3) / 2, 0, 3.0/2], 
               [sqrt(3) / 3, -1 / 3, 0, 2.0/3, 0.5]]

class Px:
    def __init__(self, *args):
        if(type(args[0]) is tuple):
            self.x = args[0][0]
            self.y = args[0][1]
            self.z = 0 if len(args) < 2 else args[1]
        else:
            self.x = args[0]
            self.y = args[1]
            self.z = 0 if len(args) < 3 else args[2]

    def __hash__(self): return hash((self.x,self.y,self.z))
    def __eq__(self,other): return self.x==other.x and self.y==other.y and self.z==other.z

    def into_hx(self):
        px = Px(self.x / (Tile.SIZE), self.y / (Tile.ISO_SCALE * Tile.SIZE), self.z)
        q = ORIENTATION[1][0] * px.x + ORIENTATION[1][1] * px.y
        r = ORIENTATION[1][2] * px.x + ORIENTATION[1][3] * px.y
        return self.hex_round(q,r,self.z)

    def hex_round(self, aq, ar, az):
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
        return Hx(q, r, az)

class Hx:
    def __init__(self, *args):
        if(type(args[0]) is tuple):
            self.q = args[0][0]
            self.r = args[0][1]
            self.z = 0 if len(args) < 2 else args[1]
        else:
            self.q = args[0]
            self.r = args[1]
            self.z = 0 if len(args) < 3 else args[2]
    
    def __hash__(self): return hash((self.q,self.r,self.z))
    def __eq__(self,other): return self.q==other.q and self.r==other.r and self.z==other.z

    @property
    def s(self): return -self.q-self.r

    def into_px(self):
        x = (ORIENTATION[0][0] * self.q + ORIENTATION[0][1] * self.r) * (Tile.SIZE)
        y = (ORIENTATION[0][2] * self.q + ORIENTATION[0][3] * self.r) * (Tile.ISO_SCALE*Tile.SIZE)
        return Px(x,y - Tile.HEIGHT/4, self.z)
    
class Tile:
    SIZE=48
    ISO_SCALE=2/3
    WIDTH=sqrt(3)*SIZE
    HEIGHT=ISO_SCALE*SIZE*2
    RISE=1/2*SIZE

    def __init__(self, z):
        self.z = z

    def hx_offset(self, corner):
        angle = 2 * pi * (0.5+corner) / 6
        return Px((Tile.SIZE) * cos(angle), (Tile.ISO_SCALE*Tile.SIZE) * sin(angle), self.z)

    def into_polygon(self):
        corners = []
        for i in range(6):
            offset = self.hx_offset(i)
            corners.append(Px(offset.x, offset.y, self.z))
        return corners