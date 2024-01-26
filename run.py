import logging
import pyglet
import sys
from pyglet.gl import glTexParameteri, GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_TEXTURE_MAG_FILTER, GL_NEAREST
from ActionBar import ActionBar

from Actor import Actor
from Assets import Assets
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

assets = Assets()

scene = Scene(assets, batch, groups)
state_manager.register(StateManager.SCENE, scene)

actor = Actor(key_state_handler, batch, groups)
state_manager.register(StateManager.ACTOR, actor)

overlay = Overlay(assets.streets[0], batch, groups[len(groups)-1])
state_manager.register(StateManager.OVERLAY,overlay)

batch_ui = pyglet.graphics.Batch()

action_bar = ActionBar(window,scene,batch=batch_ui)
state_manager.register(StateManager.ACTION_BAR,action_bar)

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
