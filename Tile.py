import random
import pyglet
import copy

from HxPx import Hx, Px, inv_hexmod
from Config import *

class Hex:
    def __init__(self, pos, sprite, group):
        self._sprite = sprite
        if sprite is not None: self._sprite.group = group
        self.group = group
        self.collider = pyglet.shapes.Polygon(*[[it.x,it.y] for it in pos.vertices()])
        self.pos = pos

    @property
    def sprite(self): return self._sprite

    @sprite.setter
    def sprite(self, v): 
        self._sprite = v
        self.pos = self.pos

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

    @property
    def hx(self): return self._hx

    @property
    def px(self): return self._px

    def delete(self):
        if self.sprite is not None: self.sprite.delete()

class Decorator(Hex):
    SIZE = TILE_SIZE/3 # TODO: calc from HXM config

    def __init__(self, pos, sprite, hxm, group):
        self._hxm = hxm
        super().__init__(pos, sprite, group)

    @Hex.pos.setter
    def pos(self, v):
        super(Decorator, self.__class__).pos.fset(self,v)
        self.adjust_pos()

    @property
    def hxm(self): return self._hxm

    @hxm.setter
    def hxm(self, v):
        self._hxm = v
        self.adjust_pos()

    def adjust_pos(self):
        offset = inv_hexmod(self.hxm)
        new_pos = (self._px.x + offset[0], self._px.y + offset[1] + self._px.z//2*RISE,0)
        self.collider.delete()
        self.collider = pyglet.shapes.Polygon(*[[it.x,it.y] for it in Px(new_pos).vertices(self.SIZE,ORIENTATION_PNTY)])
        self.sprite.position = new_pos

class Tile(Hex):
    SIZE = TILE_SIZE

    def __init__(self, pos, sprite, group):
        self.contents = [None]*HXM_A
        super().__init__(pos, sprite, group)
    
    @Hex.pos.setter
    def pos(self, v):
        super(Tile, self.__class__).pos.fset(self,v)
        self.collider.delete()
        self.collider = pyglet.shapes.Polygon(*[[it.x,it.y] for it in self._px.vertices()])
        new_pos = (self._px.x, self._px.y + self._px.z//2*RISE, self._px.z)
        if self.sprite is not None: self.sprite.position = new_pos
        for it in self.contents: 
            if it is not None: it.pos = new_pos
    
    def delete(self):
        super().delete()
        for it in self.contents: 
            if it is not None: 
                it.delete()
                it = None

class HexSet:
    def __init__(self, pos, layerset, batch, groups, hxm=None):
        self._pos = pos
        self._hxm = hxm
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
                if self.hxm is None: self._layers.append(Tile(pos, v.into_sprite(i,self.batch), self.groups[i]))
                else: self._layers.append(Decorator(pos,v.into_sprite(i,self.batch), self.hxm, self.groups[i]))

    @property
    def hxm(self): return self._hxm

    @hxm.setter
    def hxm(self, v):
        if self._hxm is None and v is None: return
        if self._hxm is not None and v is not None:
            for it in self._layers: 
                if it is not None: it.hxm = v
        else:
            for i,it in enumerate(self._layers):
                if it is not None:
                    hex = Tile(it.pos, it.sprite, it.group) if v is None else Decorator(it.pos, it.sprite, 0, it.group)
                    self._layers[i] = hex
        self._hxm = v

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
