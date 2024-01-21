import logging
import math
import pyglet
import sys

from Actor import Actor
from Camera import Camera, CenteredCamera
from Console import Console
from Scene import Scene
from StateManager import StateManager
from Tile import Px
from Overlay import Overlay

LOGLEVEL = logging.DEBUG

logging.basicConfig(stream=sys.stderr, 
                    level=LOGLEVEL, 
                    format='%(levelname)-5s %(asctime)s %(module)s:%(funcName)s %(message)s',
                    datefmt="%Y-%m-%dT%H:%M:%S")


window = pyglet.window.Window(fullscreen=False)
camera = CenteredCamera(window)
camera_ui = Camera(window)
@window.event
def on_draw():
    window.clear()
    with camera:
        batch.draw()
    with camera_ui:
        batch_ui.draw()

key_state_handler = pyglet.window.key.KeyStateHandler()
state_manager = StateManager(window, key_state_handler)

batch = pyglet.graphics.Batch()
groups = [pyglet.graphics.Group(order = i) for i in range(11)]

streets_sheet = pyglet.image.TextureGrid(pyglet.image.ImageGrid(pyglet.resource.image("assets/sprites/streets.png"),rows=4,columns=4))
for it in streets_sheet:
    it.anchor_x = it.width/2
    it.anchor_y = (3*it.height/4)/2
streets = [
    streets_sheet[2],
    streets_sheet[1],
    streets_sheet[0],
    streets_sheet[0].get_transform(flip_x=True),
    streets_sheet[5],
    streets_sheet[5].get_transform(flip_x=True),
    streets_sheet[6],
    streets_sheet[6].get_transform(flip_x=True),
    streets_sheet[7],
    streets_sheet[7].get_transform(flip_x=True),
    streets_sheet[4],
    streets_sheet[4].get_transform(flip_x=True),
    streets_sheet[8],
    streets_sheet[8].get_transform(flip_x=True),
    streets_sheet[9],
    streets_sheet[9].get_transform(flip_x=True),
    streets_sheet[10],
    streets_sheet[10].get_transform(flip_x=True),
    streets_sheet[11],
    streets_sheet[11].get_transform(flip_x=True),
    streets_sheet[14],
    streets_sheet[14].get_transform(flip_x=True),
    streets_sheet[15],
    streets_sheet[15].get_transform(flip_x=True),
    streets_sheet[12],
    streets_sheet[13],
    streets_sheet[13].get_transform(flip_x=True),
]
buildings_sheet = pyglet.image.TextureGrid(pyglet.image.ImageGrid(pyglet.resource.image("assets/sprites/buildings.png"),rows=1,columns=2))
for it in buildings_sheet:
    it.anchor_x = it.width/2
    it.anchor_y = (3*it.height/4)/2
buildings = [
    buildings_sheet[1],
    buildings_sheet[0],
]
decorators_sheet = pyglet.image.TextureGrid(pyglet.image.ImageGrid(pyglet.resource.image("assets/sprites/decorations.png"),rows=1,columns=1))
for it in decorators_sheet:
    # it.anchor_x = it.width/2
    it.anchor_y = (1*it.height/4)
decorators = [
    decorators_sheet[0],
    decorators_sheet[0].get_transform(flip_x=True)
]
scene = Scene(streets, buildings, decorators, batch, groups)
state_manager.register(StateManager.SCENE, scene)

actor = Actor(key_state_handler, batch, groups)
state_manager.register(StateManager.ACTOR, actor)

overlay = Overlay(scene, batch, groups[len(groups)-1])
state_manager.register(StateManager.OVERLAY,overlay)

batch_ui = pyglet.graphics.Batch()

console = Console(Px(window.width,window.height,0),batch=batch_ui)
state_manager.register(StateManager.CONSOLE,console)
console.toggle() # off

state_manager.begin()

def on_update(dt): 
    actor.update(dt)
    camera.position = actor.px.x, actor.px.y

pyglet.clock.schedule_interval(on_update, 1/120.0)

if __name__ == "__main__": 
    pyglet.app.run()
