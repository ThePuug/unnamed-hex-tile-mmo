from math import cos, pi, sin
from pyglet.math import Vec3

from Config import *

def hex_round(aq, ar, az):
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

class Px(Vec3):
    def __add__(self, v): return Px(*super().__add__(v))
    def __sub__(self, v): return Px(*super().__sub__(v))

    def vertices(self, tile_size = TILE_SIZE, orientation = ORIENTATION_PNTY):
        tile_size_w = TILE_SIZE_W if TILE_SIZE==tile_size else round(tile_size * sqrt(3)) / sqrt(3)
        corners = []
        for i in range(6):
            px = self
            angle = 2 * pi * (orientation[2]+i) / 6
            offset = Px(tile_size_w*cos(angle), ISO_SCALE*tile_size*sin(angle), 0)
            corners.append(px+offset)
        return corners

    def into_hx(self, tile_size = TILE_SIZE, orientation = ORIENTATION_PNTY):
        tile_size_w = TILE_SIZE_W if TILE_SIZE==tile_size else round(tile_size * sqrt(3)) / sqrt(3)
        px = Px(self.x/tile_size_w, self.y / (ISO_SCALE*tile_size), self.z)
        q = orientation[1][0] * px.x + orientation[1][1] * px.y
        r = orientation[1][2] * px.x + orientation[1][3] * px.y
        return hex_round(q,r,self.z)
    
    def into_screen(self, offset=(0,0,0)):
        hx = self.into_hx()
        pos = self + Px(0,self.z*TILE_RISE,-hx.r*100) + Px(*offset) # supports depth of 100 with 650 r values
        pos.z = (pos.z / pow(2,16)) * 255 # normalize 16 bit z to default projection depth range
        return pos
    
    @property
    def state(self): return (self.x, self.y, self.z)
    
class Hx:
    def __init__(self, *args):
        # create from position tuple
        if(type(args[0]) is tuple):
            self.q = args[0][0]
            self.r = args[0][1]
            self.z = 0 if len(args) < 2 else args[1]
        else:
            self.q = args[0]
            self.r = args[1]
            self.z = 0 if len(args) < 3 else args[2]
    
    def __hash__(self): return hash((self.q, self.r, self.z))
    def __eq__(self,other): return self.q==other.q and self.r==other.r and self.z==other.z

    def __add__(self, v): return Hx(self.q+v.q, self.r+v.r, self.z+v.z)
    def __sub__(self, v): return Hx(self.q-v.q, self.r-v.r, self.z-v.z)

    @property
    def s(self): return -self.q-self.r

    def vertices(self, tile_size=TILE_SIZE, orientation=ORIENTATION_PNTY):
        tile_size_w = TILE_SIZE_W if TILE_SIZE==tile_size else round(tile_size * sqrt(3)) / sqrt(3)
        corners = []
        for i in range(6):
            px = self.into_px()
            angle = 2*pi*(orientation[2]+i)/6
            offset = Px(tile_size_w*cos(angle), (ISO_SCALE*tile_size)*sin(angle))
            corners.append(Px(px.x+offset.x, px.y+offset.y))
        return corners

    def into_px(self, tile_size = TILE_SIZE, orientation = ORIENTATION_PNTY):
        tile_size_w = TILE_SIZE_W if TILE_SIZE==tile_size else round(tile_size * sqrt(3)) / sqrt(3)
        x = (orientation[0][0] * self.q + orientation[0][1] * self.r) * (tile_size_w)
        y = (orientation[0][2] * self.q + orientation[0][3] * self.r) * (ISO_SCALE*tile_size)
        return Px(x, y, self.z)
    
    # def into_screen(self, offset=(0,0,0)):
    #     px = self.into_px()
    #     return (px.x+offset[0], px.y+px.z*TILE_RISE+offset[1], -self.r+offset[2])
    
    @property
    def state(self): return (self.q, self.r, self.z)
    

