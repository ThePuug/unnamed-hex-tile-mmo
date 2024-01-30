import pyglet
from pyglet.gl import glTexParameteri, GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_TEXTURE_MAG_FILTER, GL_NEAREST

from HxPx import Hx

class LayerSet:
    def __init__(self, layers, scale):
        self.layers = layers if layers is not None else []
        self.scale = scale if scale is not None else (0,0)

    def into_sprite(self, z, batch):
        if self.layers[z] is None: return None
        it = pyglet.sprite.Sprite(self.layers[z],batch=batch)
        it.scale_x = self.scale[0]
        it.scale_y = self.scale[1]
        return it

class Assets:
    MAX_HEIGHT = 1
    def __init__(self):
        hx = Hx(0,0,0)
        self.streets = self.load("assets/sprites/streets.png",(4,4),(1,3/4),[
            [(2,False)],[(1,False)],[(0,False)],[(0,True)],[(5,False)],[(5,True)],[(6,False)],[(6,True)],[(7,False)],[(7,True)],[(4,False)],[(4,True)],[(8,False)],[(8,True)],
            [(9,False)],[(9,True)],[(10,False)],[(10,True)],[(11,False)],[(11,True)],[(14,False)],[(14,True)],[(15,False)],[(15,True)],[(12,False)],[(13,False)],[(13,True)]
            ],(hx.width/(83),hx.height/(3/4*128)))
        self.terrain = self.load("assets/sprites/terrain.png",(5,1),(1,3/4),None,(hx.width/(83),hx.height/(3/4*128)))
        self.buildings = self.load("assets/sprites/buildings.png",(2,1),(1,3/4),[[None,(1,False),(0,False)]],(hx.width/(83),hx.height/(3/4*128)))
        self.decorators = self.load("assets/sprites/decorators.png",(2,1),(1,1),[[None,(0,False),None,(1,False)]],((hx.width/3)/(27),(hx.height/3)/(1/2*64)))
        Assets.MAX_HEIGHT = max([max([len(set.layers) for set in set]) for set in [self.streets,self.terrain,self.buildings,self.decorators]])

    def load(self, img, grid_size, anchor_factor = None, order = None, scale = None):
        sheet = pyglet.image.TextureGrid(pyglet.image.ImageGrid(pyglet.resource.image(img),rows=grid_size[1],columns=grid_size[0]))
        glTexParameteri(GL_TEXTURE_2D,GL_TEXTURE_MIN_FILTER,GL_NEAREST)
        glTexParameteri(GL_TEXTURE_2D,GL_TEXTURE_MAG_FILTER,GL_NEAREST)
        for it in sheet:
            it.anchor_x = (1 if anchor_factor is None else anchor_factor[0])*it.width/2
            it.anchor_y = (1 if anchor_factor is None else anchor_factor[1])*it.height/2
        if order is None: order = [[(it,False)] for it in range(len(sheet))]
        imgs = [ [None if z is None else sheet[z[0]].get_transform(flip_x=z[1]) for z in it] for it in order ]
        return [LayerSet(layers,scale) for layers in imgs]
