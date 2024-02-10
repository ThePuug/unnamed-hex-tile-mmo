from collections import deque
import logging
import socket
import pyglet
import sys

from ActionBar import ActionBar
import Actor
from Assets import Assets
from Camera import Camera, CenteredCamera
from Config import *
from Scene import Scene
from Session import Session
import StateManager
from Overlay import Overlay

logging.basicConfig(stream=sys.stderr, 
                    level=LOGLEVEL, 
                    format='%(levelname)-5s %(asctime)s %(module)s:%(funcName)s %(message)s',
                    datefmt="%Y-%m-%dT%H:%M:%S")

pyglet.resource.path = ['assets/sprites','data/maps']
pyglet.resource.reindex()

window = pyglet.window.Window(fullscreen=False)

fps = pyglet.window.FPSDisplay(window=window)
camera = CenteredCamera(window)
camera_ui = Camera(window)

sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
sock.connect((SERVER,SERVER_PORT))
session = Session(sock, deque(), deque())

key_state_handler = pyglet.window.key.KeyStateHandler()
batch = pyglet.graphics.Batch()
batch_ui = pyglet.graphics.Batch()

state_manager = StateManager.StateManager(session, window, key_state_handler)
scene = Scene(Assets(), Actor.Factory(key_state_handler, batch), state_manager, batch)
overlay = Overlay(batch)
action_bar = ActionBar(window,scene,batch=batch_ui)

state_manager.register(StateManager.SCENE, scene)
state_manager.register(StateManager.OVERLAY,overlay)
state_manager.register(StateManager.ACTION_BAR,action_bar)

state_manager.begin()

@window.event
def on_draw():
    window.clear()
    with camera:
        batch.draw()
    with camera_ui:
        batch_ui.draw()
        fps.draw()

def on_update(dt):
    state_manager.update(dt)
    if state_manager.tid is not None:
        actor = state_manager.registry[StateManager.SCENE].actors.get(state_manager.tid)
        if actor is not None:
            actor.update(dt)
            camera.position = actor.px.into_screen()[:2]
pyglet.clock.schedule_interval(on_update, 1/120.0)

if __name__ == "__main__": 
    pyglet.app.run()
