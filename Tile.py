import copy
from HxPx import Hx, Px, inv_hexmod

class Tile:
    RISE=24

    def __init__(self, pos, sprite, group):
        self.contents = [None]*7
        self.sprite = sprite
        if sprite is not None: self.sprite.group = group
        self.pos = pos

    @property
    def pos(self): return self._pos

    @pos.setter
    def pos(self, v):
        self._pos = v
        if type(v) is Hx: 
            self._hx = v
            self._px = v.into_px()
        elif type(v) is Px: 
            self._hx = v.into_hx()
            self._px = v
        self.update_position()

    @property
    def hx(self): return self._hx

    @property
    def px(self): return self._px

    def delete(self):
        if self.sprite is not None: self.sprite.delete()
        for it in self.contents: 
            if it is not None: 
                it.delete()
                it = None
                
    def update_position(self):
        new_pos = (self._px.x, self._px.y + self._px.z//2*Tile.RISE, self._px.z)
        if self.sprite is not None: 
            self.sprite.position = new_pos
        for i,it in enumerate(self.contents): 
            if it is not None: 
                offset = inv_hexmod(i)
                px = Px(new_pos[0]+offset[0], new_pos[1]+offset[1], 0)
                it.pos = px
        
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
                self._layers[0].delete()
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
