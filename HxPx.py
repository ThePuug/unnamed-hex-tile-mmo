from math import cos, floor, pi, sin, sqrt

TRANSFORM_MD = (48,3/4,(0,0))
TRANSFORM_SM = (16,3/4,(0,0))

HXM_R = 1
HXM_S = 3*HXM_R + 2
HXM_A = 3*HXM_R^2 + 3*HXM_R + 1

def hexmod(pos):
  x,y=pos
  # todo scale x,y against tile_size
  m = (y+HXM_S*x)//HXM_A
  return m

def inv_hexmod(m): 
    ms = (m+HXM_R)//HXM_S
    mcs = (m+2*HXM_R)//(HXM_S-1)
    q = ms*(HXM_R+1) + mcs*-HXM_R
    r = m + ms*(-2*HXM_R-1) + mcs*(-HXM_R-1)
    # z = -m + ms*HXM_R + mcs*(2*HXM_R+1) # what is z for?
    hx = Hx(q,r,0)
    hx.transform = TRANSFORM_SM
    px = hx.into_px()
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

ORIENTATION = [sqrt(3), sqrt(3) / 2, 0, 3.0/2]
CONSTS = [1/sqrt(3), sqrt(3)]

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
        self.transform = TRANSFORM_MD

    def __hash__(self): return hash((self.x,self.y,self.z))
    def __eq__(self,other): return self.x==other.x and self.y==other.y and self.z==other.z

    @property
    def transform(self): return (self._tile_size, self._iso_scale, self._offset)

    @transform.setter
    def transform(self, v):
        if v[0] is not None: self._tile_size = v[0]
        if v[1] is not None: self._iso_scale = v[1]
        if v[2] is not None: self._offset = v[2]

    @property
    def tile_size(self): return self._tile_size

    @property
    def iso_scale(self): return self._iso_scale

    @property
    def offset(self): return self._offset

    def into_hx(self):
        x = self.x/self._tile_size*CONSTS[0]
        y = self.y/(self.iso_scale*self._tile_size)*-CONSTS[0]
        t = CONSTS[1] * y + 1           # scaled y, plus phase
        temp1 = floor( t + x )          # (y+x) diagonal, this calc needs floor
        temp2 = ( t - x )               # (y-x) diagonal, no floor needed
        temp3 = ( 2 * x + 1 )           # scaled horizontal, no floor needed, needs +1 to get correct phase
        qf = (temp1 + temp3) / 3.0      # pseudo x with fraction
        rf = (temp1 + temp2) / 3.0      # pseudo y with fraction
        q = floor(qf)                   # pseudo x, quantized and thus requires floor
        r = -floor(rf)                  # pseudo y, quantized and thus requires floor
        return Hx(q, r, self.z)

class Hx:
    def __init__(self, q, r, z):
        self.q = q
        self.r = r
        self.z = z
        self.transform = TRANSFORM_MD
    
    def __hash__(self): return hash((self.q,self.r,self.z))
    def __eq__(self,other): return self.q==other.q and self.r==other.r and self.z==other.z

    @property
    def vertices(self):
        corners = []
        for i in range(6):
            px = self.into_px()
            angle = 2 * pi * (0.5+i) / 6
            offset = Px(px.tile_size * cos(angle), (px.iso_scale*px.tile_size) * sin(angle))
            corners.append(Px(px.x+offset.x, px.y+offset.y))
        return corners

    @property
    def s(self): return -self.q-self.r

    def into_px(self):
        x = (ORIENTATION[0] * self.q + ORIENTATION[1] * self.r) * (self._tile_size)
        y = (ORIENTATION[2] * self.q + ORIENTATION[3] * self.r) * (self._iso_scale*self._tile_size)
        return Px(x + self._offset[0], y + self._offset[1], self.z)
    
    @property
    def transform(self): return (self._tile_size, self._iso_scale, self._offset)

    @transform.setter
    def transform(self, v):
        if v[0] is not None: self._tile_size = v[0]
        if v[1] is not None: self._iso_scale = v[1]
        if v[2] is not None: self._offset = v[2]
        self._width = sqrt(3)*self.tile_size
        self._height = self._iso_scale*self._tile_size*2

    @property
    def tile_size(self): return self._tile_size

    @property
    def iso_scale(self): return self._iso_scale

    @property
    def offset(self): return self._offset

    @property
    def width(self): return self._width

    @property
    def height(self): return self._height
