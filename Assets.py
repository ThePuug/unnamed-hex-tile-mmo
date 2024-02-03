import pyglet
from pyglet.graphics import *
from Config import *
from HxPx import Px
from Tile import Tile

fragment_source = """#version 150 core
    in vec4 vertex_colors;
    in vec3 texture_coords;
    out vec4 final_colors;

    uniform sampler2D sprite_texture;

    void main()
    {
        final_colors = texture(sprite_texture, texture_coords.xy) * vertex_colors;
        
        // No GL_ALPHA_TEST in core, use shader to discard.
        if(final_colors.a < 0.01){
            discard;
        }
    }
"""

class DepthSpriteGroup(pyglet.sprite.SpriteGroup):
    def set_state(self):
        self.program.use()

        glActiveTexture(GL_TEXTURE0)
        glBindTexture(self.texture.target, self.texture.id)

        glEnable(GL_BLEND)
        glBlendFunc(self.blend_src, self.blend_dest)

        glEnable(GL_DEPTH_TEST)
        glDepthFunc(GL_LESS)

    def unset_state(self):
        glDisable(GL_BLEND)
        glDisable(GL_DEPTH_TEST)
        self.program.stop()

class DepthSprite(pyglet.sprite.AdvancedSprite):
    group_class = DepthSpriteGroup

# Re-use vertex source and create new shader with alpha testing.
vertex_shader = pyglet.graphics.shader.Shader(pyglet.sprite.vertex_source, "vertex")
fragment_shader = pyglet.graphics.shader.Shader(fragment_source, "fragment")
depth_shader = pyglet.graphics.shader.ShaderProgram(vertex_shader, fragment_shader)

class TileFactory:
    def __init__(self, img, scale, flags):
        self.img = img
        self.scale = scale
        self.flags = flags

    def create(self, pos, batch):
        sprite = DepthSprite(self.img,batch=batch,program=depth_shader)
        sprite.scale_x = TILE_WIDTH / (self.img.width * self.scale.x)
        sprite.scale_y *= TILE_HEIGHT / (self.img.height * self.scale.y)
        return Tile(pos, sprite, self.flags, batch)

class Assets:
    def __init__(self):
        pyglet.resource.path = ['assets/sprites']
        pyglet.resource.reindex()
        #                           filename,         grid,  anchors, order, scale,     flags 
        self.terrain    = self.load("terrain.png",    (5,1), (1,1),   None,  Px(1,1),   FLAG_SOLID)
        self.buildings  = self.load("buildings.png",  (1,1), (1,5/4), None,  Px(1,3/4), FLAG_SOLID)
        self.decorators = self.load("decorators.png", (1,1), (1,1/3), None,  Px(1,1/3))

    def load(self, img, grid_size, anchor_factor = None, order = None, scale = Px(1,1), flags = 0):
        sheet = pyglet.image.TextureGrid(pyglet.image.ImageGrid(pyglet.resource.image(img),rows=grid_size[1],columns=grid_size[0]))
        for it in sheet:
            it.anchor_x = (1 if anchor_factor is None else anchor_factor[0])*it.width/2
            it.anchor_y = (1 if anchor_factor is None else anchor_factor[1])*it.height/2
        if order is None: order = [(it,False) for it in range(len(sheet))]
        return [TileFactory(sheet[it[0]].get_transform(flip_x=it[1]), scale, flags) for it in order]
