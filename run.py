import logging
import pyglet
import sys

from Actor import Actor
from Camera import Camera, CenteredCamera
from Console import Console
from Scene import Scene
from StateManager import StateManager
from Ui import Ui

LOGLEVEL = logging.DEBUG

GROUP_BACKGROUND = 1
GROUP_FOREGROUND = 2
GROUP_ACTOR = 3
GROUP_OVERLAY = 4

logging.basicConfig(stream=sys.stderr, 
                    level=LOGLEVEL, 
                    format='%(levelname)-5s %(asctime)s %(module)s:%(funcName)s %(message)s',
                    datefmt="%Y-%m-%dT%H:%M:%S")

state_manager = StateManager()

key_state_handler = pyglet.window.key.KeyStateHandler()
state_manager.register(StateManager.KEY_STATE, key_state_handler)

window = pyglet.window.Window(fullscreen=False)
state_manager.register(StateManager.WINDOW, window)

batch = pyglet.graphics.Batch()
groups = [pyglet.graphics.Group(order = 0),
          pyglet.graphics.Group(order = 1),
          pyglet.graphics.Group(order = 2),
          pyglet.graphics.Group(order = 3),
          pyglet.graphics.Group(order = 4)]

textures_tiles = pyglet.image.TextureGrid(pyglet.image.ImageGrid(pyglet.resource.image("assets/sprites/streets.png"),rows=1,columns=1))
for it in textures_tiles:
    it.anchor_x = it.width/2
    it.anchor_y = it.height/2
images_tiles = {
    'green': textures_tiles[0],
    # 'bend_r-y': textures_tiles[8],
    # 'bend_r+y': textures_tiles[5],
    # 'straight_r': textures_tiles[7],
    # 'straight_qs': textures_tiles[3],
    # 'straight+x': textures_tiles[2],
    # 'corner+x': textures_tiles[1],
    # 'corner+y': textures_tiles[4],
    # 'corner-y': textures_tiles[0],
}
scene = Scene(images_tiles, batch, groups)
state_manager.register(StateManager.SCENE, scene)

# generate and anchor actor frames at bottom center
frames_blank = pyglet.image.ImageGrid(pyglet.resource.image("assets/sprites/blank.png"),rows=4,columns=4)
for it in frames_blank:
    it.anchor_x = 31
    it.anchor_y = 5
animations = {
    "walk_n": pyglet.image.Animation.from_image_sequence([frames_blank[i] for i in [1,2,3,0]], duration=0.4),
    "walk_e": pyglet.image.Animation.from_image_sequence([frames_blank[i] for i in [5,6,7,4]], duration=0.4),
    "walk_w": pyglet.image.Animation.from_image_sequence([frames_blank[i] for i in [9,10,11,8]], duration=0.4),
    "walk_s": pyglet.image.Animation.from_image_sequence([frames_blank[i] for i in [13,14,15,12]], duration=0.4),
    "stand_n": pyglet.image.Animation.from_image_sequence([frames_blank[i] for i in [0,2]], duration=0.4),
    "stand_e": pyglet.image.Animation.from_image_sequence([frames_blank[i] for i in [4,6]], duration=0.4),
    "stand_w": pyglet.image.Animation.from_image_sequence([frames_blank[i] for i in [8,10]], duration=0.4),
    "stand_s": pyglet.image.Animation.from_image_sequence([frames_blank[i] for i in [12,14]], duration=0.4)}
actor = Actor(pyglet.sprite.Sprite(animations["walk_s"], group=groups[GROUP_ACTOR], batch=batch), animations, key_state_handler)
state_manager.register(StateManager.ACTOR,actor)

batch_ui = pyglet.graphics.Batch()

console = Console("",50,50,window.width-100,batch=batch_ui)
state_manager.register(StateManager.CONSOLE,console)

ui = Ui(console)
state_manager.register(StateManager.UI,ui)

state_manager.begin(window)

camera = CenteredCamera(window)
camera_ui = Camera(window)
@window.event
def on_draw():
    window.clear()
    with camera:
        batch.draw()
    with camera_ui:
        batch_ui.draw()

them = [actor,scene]
def on_update(dt): 
    for it in them: it.update(dt)
    camera.position = actor.px.x, actor.px.y
    scene.highlight_at(actor.px)

pyglet.clock.schedule_interval(on_update, 1/120.0)

if __name__ == "__main__": 
    pyglet.app.run()
