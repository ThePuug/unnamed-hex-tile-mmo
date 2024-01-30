from logging import debug
import pyglet

from HxPx import Hx, inv_hexmod
from Tile import Tile

R=5
NEIGHBORS = [Hx(+1,0,0),Hx(+1,-1,0),Hx(0,-1,0),Hx(-1,0,0),Hx(-1,+1,0),Hx(0,+1,0)]

class Scene(pyglet.event.EventDispatcher):

    def __init__(self, assets, batch, groups):
        self.terrain = assets.terrain
        self.streets = assets.streets
        self.buildings = assets.buildings
        self.decorators = assets.decorators
        self.batch = batch
        self.groups = groups
        self.tiles = {}
        self.decorations = {}

    def on_looking_at(self, hx):
        if self.tiles.get(hx,None) is None:
            self.dispatch_event('on_discover',hx)

    def on_select(self, hx, hxm, layerset):
        for i,it in enumerate(layerset.layers):
            if it is None: continue
            hxi = Hx(hx.q,hx.r,hx.z+i)
            tile = self.tiles.get(hxi)
            sprite = layerset.into_sprite(i,self.batch)
            if hxm is None:
                if tile is not None:
                    self.tiles[hxi].delete()
                    del self.tiles[hxi]
                self.tiles[hxi] = Tile(hxi,sprite,self.groups[hxi.z])
            else:
                if tile is None:
                    tile = Tile(hxi,None,self.groups[hxi.z])
                    self.tiles[hxi] = tile
                if tile.contents[hxm] is not None:
                    tile.contents[hxm].delete()
                tile.contents[hxm] = Tile(hxi,sprite,self.groups[hxi.z])
                tile.update_position()
                

    def on_discover(self, c):
        for q in range(-R, R+1):
            r1 = max(-R, -q-R)
            r2 = min( R, -q+R)
            for r in range(r1,r2+1):
                hx = Hx(c.q + q, c.r + r, c.z)
                if not(self.tiles.get(hx,None) is None): continue
                self.tiles[hx] = Tile(hx, self.terrain[0].into_sprite(0,self.batch), self.groups[hx.z])
    
    def on_move_to(self, actor, px):
        hx = px.into_hx()
        for tile in [self.tiles.get(Hx(hx.q+offset.q,hx.r+offset.r,hx.z)) for offset in NEIGHBORS + [hx]]:
            for it in (tile.contents if tile is not None else [])+[tile]:
                if it is not None and it.sprite is not None:
                    pass
                    # debug("check: tlrd({},{},{},{})".format(it.sprite.y+it.sprite.height, it.sprite.x-it.sprite.width, it.sprite.x+it.sprite.width, it.sprite.y))

    # def collides_with(self, actor):
    #     r1 = collision[entity1].aabb
    #     r2 = collision[entity2].aabb
    #     sprite1 = renderable[entity1].sprite
    #     sprite2 = renderable[entity2].sprite
    #     ri = r1.intersect(r2)


    #     offx1, offy1 = int(ri.left - r1.left), int(ri.top - r1.top)
    #     offx2, offy2 = int(ri.left - r2.left), int(ri.top - r2.top)

    #     d1 = sprite1.texture.get_image_data().get_data('A', sprite1.texture.width)
    #     d2 = sprite2.texture.get_image_data().get_data('A', sprite2.texture.width)

    #     p1 = cast(d1, POINTER(c_ubyte))
    #     p2 = cast(d2, POINTER(c_ubyte))

    #     for i in range(0, int(ri.width)):
    #         for j in range(0, int(ri.height)):
    #             c1, c2 = 0, 0
    #             x1 = offx1+i
    #             y1 = (j+offy1)*sprite1.texture.width
    #             x2 = offx2+i
    #             y2 = (offy2+j)*sprite2.texture.width

    #             if x1 >= 0 and y1 >= 0:
    #                 c1 = p1[x1 + y1]

    #             if x2 >= 0 and y2 >= 0:
    #                 c2 = p2[x2 + y2]

    #             if c1>0 and c2 >0:
    #                 pairs.add(pair)
    #                 break

Scene.register_event_type('on_discover')