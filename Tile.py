class Tile:
    RISE=24

    def __init__(self, hx, sprite, group):
        self.hx = hx
        self.sprite = sprite
        self.sprite.group = group
        self.px = hx.into_px()
    
    @property
    def px(self): self._px

    @px.setter
    def px(self, v):
        self._px = v
        self._hx = v.into_hx()
        self.sprite.position = (v.x, v.y + self._hx.height/4 + v.z//2*Tile.RISE, v.z)
        
class TileSet:
    def __init__(self, hx, layerset, batch, groups):
        self._layers = []
        self.hx = hx
        self.layers = layerset
        self.batch = batch
        self.groups = groups

    @property
    def layers(self): return self._layers

    @layers.setter
    def layers(self, v):
        for i in range(len(self._layers)): 
            if self._layers[len(v.layers)] is not None:
                self._layers[len(v.layers)].delete()
            del self._layers[len(v.layers)]
        for i in range(len(self._layers)): 
            if v.layers[i] is None:
                self._layers.append(None)
            else:
                self._layers.append(Tile(self.hx, v.into_sprite(i,self.batch), self.groups[i]))
