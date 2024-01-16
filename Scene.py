from math import sqrt
import math
import pyglet

from Tile import Hx, Px

TILE_SIZE=18
TILE_WIDTH=math.ceil(sqrt(3)*TILE_SIZE)
R=10

class Scene(pyglet.event.EventDispatcher):
    def __init__(self, textures, batch, groups):
        self.textures = textures
        self.batch = batch
        self.groups = groups
        self.tiles = {}
        self.actor_at = Hx(0,0)
        self.dispatch_event('on_discover',self.actor_at)


    def highlight_at(self,px):
        hx = px.into_hx(TILE_SIZE)
        if self.actor_at == hx: return 
        last = self.tiles.get(self.actor_at,None)
        if not last is None: 
            last.scale = (TILE_WIDTH/self.textures["green"].width)
            last.group = self.groups[1]

        curr = self.tiles.get(hx,None)
        if curr is None: return
        self.dispatch_event('on_discover',hx)
        self.actor_at = hx
        curr.scale *= 1.2
        curr.group = self.groups[2]

    def draw(self):
        self._batch.draw()

    def update(self,dt):
        pass
    
    def on_discover(self, c):
        for q in range(-R, R+1):
            r1 = max(-R, -q-R)
            r2 = min( R, -q+R)
            for r in range(r1,r2+1):
                hx = Hx(c.q + q, c.r + r)
                if not(self.tiles.get(hx,None) is None): continue
                # tile = pyglet.shapes.Polygon(*[[it.x,it.y] for it in Tile(TILE_SIZE).into_polygon()],
                #                              color=tuple(random.choices(list(range(150,255)), k=3)),
                #                              batch=self._batch)
                tile = pyglet.sprite.Sprite(img=self.textures['green'],batch=self.batch,group=self.groups[1])
                tile.scale = TILE_WIDTH/self.textures["green"].width
                px = hx.into_px(TILE_SIZE)
                tile.x = px.x
                tile.y = px.y+TILE_SIZE
                self.tiles[hx]=tile

Scene.register_event_type('on_discover')