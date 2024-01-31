from math import cos, floor, pi, sin, sqrt

from Config import *

def hexmod(pos):
  x,y=pos
  # TODO: scale x,y against tile_size and iso_scale
  m = (y+HXM_S*x)//HXM_A
  return m

def inv_hexmod(m): 
    ms = (m+HXM_R)//HXM_S
    mcs = (m+2*HXM_R)//(HXM_S-1)
    q = ms*(HXM_R+1) + mcs*-HXM_R
    r = m + ms*(-2*HXM_R-1) + mcs*(-HXM_R-1)
    # z = -m + ms*HXM_R + mcs*(2*HXM_R+1) # what is z for?
    hx = Hx(q,r,0)
    px = hx.into_px(TILE_SIZE / 3, ORIENTATION_PNTY)
    return (px.x,px.y)

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

class Px:
    def __init__(self, *args):
        # create from position tuple
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

    def vertices(self, tile_size = TILE_SIZE, orientation = ORIENTATION_PNTY):
        corners = []
        for i in range(6):
            px = self
            angle = 2 * pi * (orientation[2]+i) / 6
            offset = Px(tile_size*cos(angle), ISO_SCALE*tile_size*sin(angle))
            corners.append(Px(px.x+offset.x, px.y+offset.y))
        return corners

    def into_hx(self, tile_size = TILE_SIZE, orientation = ORIENTATION_PNTY):
        px = Px(self.x/tile_size, self.y / (ISO_SCALE*tile_size), self.z)
        q = orientation[1][0] * px.x + orientation[1][1] * px.y
        r = orientation[1][2] * px.x + orientation[1][3] * px.y
        return hex_round(q,r,self.z)

    # def into_hx(self, tile_size = TILE_SIZE):
    #     x = self.x/tile_size*ONE_SQRT3
    #     y = self.y/(ISO_SCALE*tile_size)*-ONE_SQRT3
    #     t = SQRT3 * y + 1               # scaled y, plus phase
    #     temp1 = floor( t + x )          # (y+x) diagonal, this calc needs floor
    #     temp2 = ( t - x )               # (y-x) diagonal, no floor needed
    #     temp3 = ( 2 * x + 1 )           # scaled horizontal, no floor needed, needs +1 to get correct phase
    #     qf = (temp1 + temp3) / 3.0      # pseudo x with fraction
    #     rf = (temp1 + temp2) / 3.0      # pseudo y with fraction
    #     q = floor(qf)                   # pseudo x, quantized and thus requires floor
    #     r = -floor(rf)                  # pseudo y, quantized and thus requires floor
    #     return Hx(q, r, self.z)
    
class Hx:
    def __init__(self, q, r, z):
        self.q = q
        self.r = r
        self.z = z
    
    def __hash__(self): return hash((self.q,self.r,self.z))
    def __eq__(self,other): return self.q==other.q and self.r==other.r and self.z==other.z

    @property
    def s(self): return -self.q-self.r

    def vertices(self, tile_size = TILE_SIZE, orientation = ORIENTATION_PNTY):
        corners = []
        for i in range(6):
            px = self.into_px()
            angle = 2 * pi * (orientation[2]+i) / 6
            offset = Px(tile_size * cos(angle), (ISO_SCALE*tile_size) * sin(angle))
            corners.append(Px(px.x+offset.x, px.y+offset.y))
        return corners

    def into_px(self, tile_size = TILE_SIZE, orientation = ORIENTATION_PNTY):
        x = (orientation[0][0] * self.q + orientation[0][1] * self.r) * (tile_size)
        y = (orientation[0][2] * self.q + orientation[0][3] * self.r) * (ISO_SCALE*tile_size)
        return Px(x, y, self.z)
