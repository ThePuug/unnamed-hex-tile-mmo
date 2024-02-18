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

class Server:
    def __init__(self):
        self.incoming = deque()
        self.clients = {}

        self.sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.sock.bind(("localhost",SERVER_PORT))
        self.sock.listen(5)

    def accept(self):
        while True:
            info("ready to accept a connection")
            sock, addr = self.sock.accept()
            info("accepted from {}".format(addr))
            sock.send(OK)
            session = Session(sock, self.incoming, deque())
            self.clients[session.tid] = session
    
    def recv(self):
        while self.incoming: yield self.incoming.popleft()
    
    def send(self, evt, tid, seq):
        if tid == None:
            for i,it in self.clients.items(): it.send(evt, i, seq)
        else:
            it = self.clients.get(tid)
            if it is not None: it.send(evt, tid, seq)

logging.basicConfig(stream=sys.stderr, 
                    level=LOGLEVEL, 
                    format='%(levelname)-5s %(asctime)s %(module)s:%(funcName)s %(message)s',
                    datefmt="%Y-%m-%dT%H:%M:%S")

pyglet.resource.path = ['assets/sprites','data/maps']
pyglet.resource.reindex()

server = Server()
thread = threading.Thread(target=Server.accept, args=[server])
thread.daemon = True
thread.start()

state_manager = StateManager.Impl(server)

scene = Scene.Impl(Asset.Factory(), Actor.ImplFactory(), state_manager, None)
state_manager.register(StateManager.SCENE, scene)

state_manager.begin()

try:
    scene.tiles = scene.from_file()
except Exception as e:
    debug(e)
    state_manager.dispatch_event("on_try", None, TileDiscoverEvent((0,0,0)))

def on_update(dt):
    state_manager.update(dt)
    for i,it in server.clients.items():
        if it.do_exit.is_set(): 
            state_manager.dispatch_event('on_do', None, ActorUnloadEvent(i))
            it.sock.close()
            del server.clients[i]
            break
    for i,it in scene.actors.items(): it.update(it.state,dt)
pyglet.clock.schedule_interval(on_update, 1/20.0)

if __name__ == "__main__": 
    signal.signal(signal.SIGINT, lambda sig,frame: state_manager.dispatch_event('on_close'))
    pyglet.app.run()
