import pyglet
from pyglet.gl import glTexParameteri, GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_TEXTURE_MAG_FILTER, GL_NEAREST

from Tile import Px

class Assets:
    def __init__(self):
        self.streets = self.load("assets/sprites/streets.png",Px(4,4,0),Px(1,3/4,0),[
            (2,False),(1,False),(0,False),(0,True),(5,False),(5,True),(6,False),(6,True),(7,False),(7,True),(4,False),(4,True),(8,False),(8,True),
            (9,False),(9,True),(10,False),(10,True),(11,False),(11,True),(14,False),(14,True),(15,False),(15,True),(12,False),(13,False),(13,True)
            ])
        self.terrain = self.load("assets/sprites/terrain.png",Px(5,1,0),Px(1,3/4,0))
        self.buildings = self.load("assets/sprites/buildings.png",Px(2,1,0),Px(1,3/4,0),[(1,False),(0,False)])
        self.decorators = self.load("assets/sprites/decorations.png",Px(1,1,0),Px(0,1/2,0),[(0,False),(0,True)])

    def load(self, img, grid_size, anchor_factor = None, order = None):
        sheet = pyglet.image.TextureGrid(pyglet.image.ImageGrid(pyglet.resource.image(img),rows=grid_size.y,columns=grid_size.x))
        glTexParameteri(GL_TEXTURE_2D,GL_TEXTURE_MIN_FILTER,GL_NEAREST)
        glTexParameteri(GL_TEXTURE_2D,GL_TEXTURE_MAG_FILTER,GL_NEAREST)
        for it in sheet:
            it.anchor_x = (1 if anchor_factor is None else anchor_factor.x)*it.width/2
            it.anchor_y = (1 if anchor_factor is None else anchor_factor.y)*it.height/2
        if order is None: order = zip(range(len(sheet)),[False for _ in range(len(sheet))])
        imgs = [ sheet[it[0]].get_transform(flip_x=it[1]) for it in order ]
        return imgs
