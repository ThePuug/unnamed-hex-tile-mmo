import copy
from HxPx import Hx, Px


class Tile:
    RISE=24

    def __init__(self, pos, sprite, group):
        self.sprite = sprite
        self.sprite.group = group
        self.pos = pos

    @property
    def pos(self): return self._pos

    @pos.setter
    def pos(self, v):
        self._pos = v
        if type(v) is Hx: 
            self._hx = v
            self.px = v.into_px()
        elif type(v) is Px: 
            self.px = v
            self._hx = v.into_hx()

    @property
    def hx(self): return self._hx

    @hx.setter
    def hx(self, v):
        self._hx = v
        self.px = v.into_px()

    @property
    def px(self): return self._px

    @px.setter
    def px(self, v):
        self._px = v
        self._hx = v.into_hx()
        self.sprite.position = (v.x, v.y + self._hx.height/4 + v.z//2*Tile.RISE, v.z)
        
class TileSet:
    def __init__(self, pos, layerset, batch, groups):
        self._pos = pos
        self.batch = batch
        self.groups = groups

        self._layers = []
        self.layers = layerset

    @property
    def pos(self): return self._pos

    @pos.setter
    def pos(self, v):
        self._pos = v
        for i,it in enumerate(self._layers):
            if(it is not None):
                if(type(v) is Hx): it.pos = Hx(v.q, v.r, v.z+i)
                if(type(v) is Px): it.pos = Px(v.x, v.y, v.z+i)

    @property
    def layers(self): return self._layers

    @layers.setter
    def layers(self, v):
        for i in range(len(self._layers)): 
            if self._layers[0] is not None:
                self._layers[0].sprite.delete()
            del self._layers[0]
        for i in range(len(v.layers)):
            if v.layers[i] is None:
                self._layers.append(None)
            else:
                pos = copy.copy(self._pos)
                pos.z += i
                self._layers.append(Tile(pos, v.into_sprite(i,self.batch), self.groups[i]))
    
    @property
    def visible(self):
        if len(self.layers) == 0: return False
        else: 
            for it in self.layers: 
                if it is not None: return it.sprite.visible

    @visible.setter
    def visible(self, v):
        for it in self.layers:
            if it is not None: it.sprite.visible = v
