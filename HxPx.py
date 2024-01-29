from math import cos, pi, sin, sqrt

DEFAULT_SIZE = 48
DEFAULT_ISO_SCALE = 3/4

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

ORIENTATION = [[sqrt(3), sqrt(3) / 2, 0, 3.0/2], 
               [sqrt(3) / 3, -1 / 3, 0, 2.0/3, 0.5]]

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
        self.tile_size = DEFAULT_SIZE
        self.iso_scale = DEFAULT_ISO_SCALE

    def __hash__(self): return hash((self.x,self.y,self.z))
    def __eq__(self,other): return self.x==other.x and self.y==other.y and self.z==other.z

    def into_hx(self):
        px = Px(self.x / (self.tile_size), self.y / (self.iso_scale * self.tile_size), self.z)
        q = ORIENTATION[1][0] * px.x + ORIENTATION[1][1] * px.y
        r = ORIENTATION[1][2] * px.x + ORIENTATION[1][3] * px.y
        return hex_round(q,r,self.z)

class Hx:
    def __init__(self, q, r, z):
        self.q = q
        self.r = r
        self.z = z
        self.tile_size = DEFAULT_SIZE
        self.iso_scale = DEFAULT_ISO_SCALE
    
    def __hash__(self): return hash((self.q,self.r,self.z))
    def __eq__(self,other): return self.q==other.q and self.r==other.r and self.z==other.z

    @property
    def vertices(self):
        corners = []
        for i in range(6):
            px = self.into_px()
            angle = 2 * pi * (0.5+i) / 6
            offset = Px(px.tile_size * cos(angle), (self.iso_scale*px.tile_size) * sin(angle))
            corners.append(Px(px.x+offset.x, px.y+offset.y))
        return corners

    @property
    def s(self): return -self.q-self.r

    def into_px(self):
        x = (ORIENTATION[0][0] * self.q + ORIENTATION[0][1] * self.r) * (self.tile_size)
        y = (ORIENTATION[0][2] * self.q + ORIENTATION[0][3] * self.r) * (self.iso_scale*self.tile_size)
        return Px(x,y - self.tile_size/2, self.z)
    
    @property
    def width(self): return sqrt(3)*self.tile_size

    @property
    def height(self): return self.iso_scale*self.tile_size*2
