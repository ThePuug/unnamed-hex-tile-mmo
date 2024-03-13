import pyglet
from pyglet.graphics import *
from pyglet.math import Vec2
from pyglet.sprite import AdvancedSprite

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

class OverlaySpriteGroup(pyglet.sprite.SpriteGroup):
    def set_state(self):
        self.program.use()

        glActiveTexture(GL_TEXTURE0)
        glBindTexture(self.texture.target, self.texture.id)

        glEnable(GL_BLEND)
        glBlendFunc(GL_SRC_ALPHA, GL_ONE_MINUS_SRC_ALPHA)

        glEnable(GL_DEPTH_TEST)
        glDepthFunc(GL_ALWAYS)
    
    def unset_state(self):
        self.program.stop()

class DepthSprite(AdvancedSprite):
    group_class = DepthSpriteGroup

class OverlaySprite(AdvancedSprite):
    group_class = OverlaySpriteGroup

# Re-use vertex source and create new shader with alpha testing.
vertex_default = pyglet.graphics.shader.Shader(pyglet.sprite.vertex_source, "vertex")
fragment_default = pyglet.graphics.shader.Shader(pyglet.sprite.fragment_source, "fragment")
fragment_shader = pyglet.graphics.shader.Shader(fragment_source, "fragment")
depth_shader = pyglet.graphics.shader.ShaderProgram(vertex_default, fragment_shader)
default_shader = pyglet.graphics.shader.ShaderProgram(vertex_default, fragment_default)

class Factory:
    def __init__(self, batch):
        self._assets = {}
        self._actors = {}
        self.batch = batch
        self.load("terrain.png", (7,1), (1,1), (1,1), (TILE_WIDTH/83,TILE_HEIGHT/96), FLAG_SOLID)
        self.load("biomes.png", (7,1), (1,4/3), (1,96/136), (TILE_WIDTH/83,TILE_HEIGHT/96), FLAG_SOLID)
        self.load("buildings.png", (1,1), (1,4/3), (1,96/136), (TILE_WIDTH/83,TILE_HEIGHT/96), FLAG_SOLID)
        self.load("decorators.png", (1,1), (1,1/3), (1,1/3), (TILE_WIDTH/27,TILE_HEIGHT*3/96), FLAG_NONE)
        self.load("ui.png", (3,1), (1,1), (0,0), (1,1), FLAG_NONE)
        self.loadActor("blank.png", 4)
        self.loadActor("dog.png", 4)

    def create_sprite(self, typ, idx, px=Px(0,0,0), **kwargs):
        batch = kwargs.pop("batch", self.batch)
        asset = self._assets[typ][idx]
        sprite = OverlaySprite(asset.texture, batch=batch, program=default_shader)
        sprite.scale_x = asset.sprite_scale[0]
        sprite.scale_y = asset.sprite_scale[1]
        sprite.position = px[:3]
        return sprite

    def create_tile(self, typ, idx, px=Px(0,0,0), flags=None, **kwargs):
        batch = kwargs.pop("batch", self.batch)
        asset = self._assets[typ][idx]
        sprite = DepthSprite(asset.texture, batch=self.batch if batch is None else batch, program=depth_shader)
        sprite._typ = typ
        sprite._idx = idx
        sprite.scale_x = (TILE_WIDTH) / (asset.texture.width * asset.scale[0])
        sprite.scale_y *= (TILE_HEIGHT) / (asset.texture.height * asset.scale[1])
        return Tile(px, sprite, flags if flags is not None else asset.flags)
    
    def create_actor(self, typ, **kwargs):
        batch = kwargs.pop("batch", self.batch)
        asset = self._actors[typ]
        sprite = DepthSprite(asset.anims["walk_s"], batch=batch, program=depth_shader)
        sprite.scale_x = asset.scale[0]
        sprite.scale_y = asset.scale[1]
        return sprite, asset.anims

    def load(self, img, grid_size, anchor_factor = None, scale = Vec2(1,1), sprite_scale = Vec2(1,1), flags = FLAG_NONE):
        typ = img[:img.index('.')]
        sheet = pyglet.image.TextureGrid(pyglet.image.ImageGrid(pyglet.resource.image(img),rows=grid_size[1],columns=grid_size[0]))
        for it in sheet:
            it.anchor_x = (1 if anchor_factor is None else anchor_factor[0])*it.width/2
            it.anchor_y = (1 if anchor_factor is None else anchor_factor[1])*it.height/2
        self._assets[typ] = [New(
                texture = sheet[i],
                scale = scale,
                sprite_scale = sprite_scale,
                flags = flags
        ) for i in range(len(sheet))]

    def loadActor(self, img, grid_width, anchor_factor = Vec2(1,0), scale = Vec2(1,1)):
        typ = img[:img.index('.')]
        frames = pyglet.image.ImageGrid(pyglet.resource.image(img), rows=3, columns=grid_width)
        for it in frames:
            it.anchor_x = anchor_factor.x*it.width/2
            it.anchor_y = anchor_factor.y*it.height/2
        self._actors[typ] = New(
            anims = {
                "walk_n": pyglet.image.Animation.from_image_sequence([frames[2,(i+1) % grid_width] for i in range(grid_width)], duration=0.4),
                "walk_e": pyglet.image.Animation.from_image_sequence([frames[1,(i+1) % grid_width].get_transform(flip_x=True) for i in range(grid_width)], duration=0.4),
                "walk_w": pyglet.image.Animation.from_image_sequence([frames[1,(i+1) % grid_width] for i in range(grid_width)], duration=0.4),
                "walk_s": pyglet.image.Animation.from_image_sequence([frames[0,(i+1) % grid_width] for i in range(grid_width)], duration=0.4),
                "stand_n": pyglet.image.Animation.from_image_sequence([frames[2,0]], duration=0.4),
                "stand_e": pyglet.image.Animation.from_image_sequence([frames[1,0].get_transform(flip_x=True)], duration=0.4),
                "stand_w": pyglet.image.Animation.from_image_sequence([frames[1,0]], duration=0.4),
                "stand_s": pyglet.image.Animation.from_image_sequence([frames[0,0]], duration=0.4)},
            scale = scale)
            
