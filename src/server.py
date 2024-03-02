from collections import deque
import logging
from logging import debug, info
import signal
import pyglet
import socket
import sys
import threading

import Actor
import Asset
from Config import *
from Event import *
import Scene
from Session import OK, Session
import StateManager

class Server(pyglet.event.EventDispatcher):
    def __init__(self):
        self.incoming = deque()
        self.sessions = {}

        self.sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.sock.bind(("0.0.0.0",SERVER_PORT))
        self.sock.listen(5)

    def accept(self):
        while True:
            info("ready to accept a connection")
            sock, addr = self.sock.accept()
            info("accepted from {}".format(addr))
            sock.send(OK)
            session = Session(sock, self.incoming, deque())
            self.sessions[session.tid] = session
    
    def on_send(self, tid, evt, seq, broadcast):
        if broadcast:
            for i,it in self.sessions.items():
                it.on_send(tid, evt, seq if it.tid == tid else None)
        else: self.sessions[tid].on_send(tid, evt, seq)

    def update(self, dt):
        while self.incoming:
            tid, evt, seq = self.incoming.popleft()
            self.dispatch_event("on_try", tid, evt, seq)

Server.register_event_type("on_try")

logging.basicConfig(stream=sys.stderr, 
                    level=LOGLEVEL, 
                    format='%(levelname)-5s %(asctime)s %(module)s:%(funcName)s %(message)s',
                    datefmt="%Y-%m-%dT%H:%M:%S")

pyglet.resource.path = ['../assets/sprites','../data/maps']
pyglet.resource.reindex()

server = Server()
thread = threading.Thread(target=Server.accept, args=[server])
thread.daemon = True
thread.start()

state_manager = StateManager.Impl()
server.push_handlers(state_manager)
state_manager.push_handlers(server)
scene = Scene.Impl(Asset.Factory(), Actor.ImplFactory(), state_manager, None)
state_manager.register(StateManager.SCENE, scene)
state_manager.begin()

try:
    scene.tiles = scene.from_file()
except Exception as e:
    debug(e)
    state_manager.dispatch_event("on_try", None, TileDiscoverEvent((0,0,0)), None)

def on_update(dt):
    server.update(dt)
    for i,it in server.sessions.items():
        if it.do_exit.is_set(): 
            state_manager.dispatch_event('on_do', None, ActorUnloadEvent(i), True)
            it.sock.close()
            del server.sessions[i]
            break
    for i,it in scene.actors.items(): it.update(it.state,dt)
pyglet.clock.schedule_interval(on_update, 1/20.0)

if __name__ == "__main__": 
    signal.signal(signal.SIGINT, lambda sig,frame: state_manager.dispatch_event('on_close'))
    pyglet.app.run()
