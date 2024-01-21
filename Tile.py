from math import cos, pi, sin, sqrt

ORIENTATION = [[sqrt(3), sqrt(3) / 2, 0, 3.0/2], 
               [sqrt(3) / 3, -1 / 3, 0, 2.0/3, 0.5]]

class Px:
    def __init__(self, x, y, z):
        self.x = x
        self.y = y
        self.z = z

    def __hash__(self): return hash((self.x,self.y,self.z))
    def __eq__(self,other): return self.x==other.x and self.y==other.y and self.z==other.z

    def into_hx(self, size: int):
        px = Px(self.x / size, self.y / size, self.z)
        q = ORIENTATION[1][0] * px.x + ORIENTATION[1][1] * px.y
        r = ORIENTATION[1][2] * px.x + ORIENTATION[1][3] * px.y
        return self.hex_round(q,r,self.z)
    
    def offset(self,x,y,z): return Hx(self.x+x,self.y+y,self.z+z)

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
    def __init__(self, q, r, z):
        self.q = q
        self.r = r
        self.z = z
    
    def __hash__(self): return hash((self.q,self.r,self.z))
    def __eq__(self,other): return self.q==other.q and self.r==other.r and self.z==other.z

    @property
    def s(self): return -self.q-self.r

    def into_px(self, size) -> Px:
        x = (ORIENTATION[0][0] * self.q + ORIENTATION[0][1] * self.r) * size
        y = (ORIENTATION[0][2] * self.q + ORIENTATION[0][3] * self.r) * size
        return Px(x,y - size/4, self.z)
    
    def offset(self,q,r,z): return Hx(self.q+q,self.r+r,self.z+z)

class Tile:
    def __init__(self, size: int, z):
        self.size = size
        self.z = z

    def hx_offset(self, corner: int) -> Px:
        angle = 2 * pi * (0.5+corner) / 6
        return Px(self.size * cos(angle), self.size * sin(angle), self.z)

    def into_polygon(self):
        corners = []
        for i in range(6):
            offset = self.hx_offset(i)
            corners.append(Px(offset.x, offset.y, self.z))
        return corners