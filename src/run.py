from collections import deque
import logging
from logging import debug
import socket
import pyglet
import sys

from ActionBar import ActionBar
import Actor
import Asset
from Camera import Camera, CenteredCamera
from Config import *
from HxPx import Px
from Scene import Scene
from Session import Session
import StateManager
from Overlay import Overlay

logging.basicConfig(stream=sys.stderr, 
                    level=LOGLEVEL, 
                    format='%(levelname)-5s %(asctime)s %(module)s:%(funcName)s %(message)s',
                    datefmt="%Y-%m-%dT%H:%M:%S")

pyglet.resource.path = ['../assets/sprites']
pyglet.resource.reindex()

window = pyglet.window.Window(fullscreen=False)

sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
sock.connect((SERVER,SERVER_PORT))

session = Session(sock, deque(), deque())
key_state_handler = pyglet.window.key.KeyStateHandler()
asset_factory = Asset.Factory()

camera = CenteredCamera(window)
batch = pyglet.graphics.Batch()
state_manager = StateManager.StateManager(window, key_state_handler, asset_factory)
state_manager.push_handlers(session)
scene = Scene(asset_factory, Actor.Factory(key_state_handler, batch), state_manager, batch)

batch_overlay = pyglet.graphics.Batch()
overlay = Overlay(asset_factory, batch_overlay)

camera_ui = Camera(window)
batch_ui = pyglet.graphics.Batch()
fps = pyglet.window.FPSDisplay(window=window)
action_bar = ActionBar(window, scene, batch_ui)

state_manager.register(StateManager.SCENE, scene)
state_manager.register(StateManager.OVERLAY, overlay)
state_manager.register(StateManager.ACTION_BAR, action_bar)

@window.event
def on_draw():
    window.clear()
    with camera:
        batch.draw()
        batch_overlay.draw()
    with camera_ui:
        batch_ui.draw()
        fps.draw()

def on_update(dt):
    for tid, evt, seq in session.recv():
        state_manager.dispatch_event("on_do", tid, evt, None, seq)
    if state_manager.tid is not None:
        actor = state_manager.registry[StateManager.SCENE].actors.get(state_manager.tid)
        if actor is not None:
            actor.update(actor.state, dt)
            camera.position = actor.px.into_screen()[:2]
    for i,it in state_manager.registry[StateManager.SCENE].actors.items():
        if it.disp_dt > 0:
            pos = Px(*(it.px.into_screen((0, it.air_dz*TILE_RISE, it.height+it.air_dz))))
            it.disp_pos = it.disp_pos.lerp(pos, min(1, dt/it.disp_dt))
            it.disp_dt = max(0, it.disp_dt-dt)
        it.recalc()

pyglet.clock.schedule_interval(on_update, 1/120.0)

if __name__ == "__main__": 
    state_manager.begin()
    pyglet.app.run()
