from collections import deque
import logging
from logging import debug
import pyglet
import socket
import sys
import threading

import Actor
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
            debug("ready to accept a connection")
            sock, addr = self.sock.accept()
            debug("accepted from {}".format(addr))
            sock.send(OK)
            session = Session(sock, self.incoming, deque())
            self.clients[session.tid] = session
    
    def recv(self):
        while self.incoming: yield self.incoming.popleft()

logging.basicConfig(stream=sys.stderr, 
                    level=LOGLEVEL, 
                    format='%(levelname)-5s %(asctime)s %(module)s:%(funcName)s %(message)s',
                    datefmt="%Y-%m-%dT%H:%M:%S")

actor_factory = Actor.ImplFactory()
state_manager = StateManager.Impl(actor_factory)

scene = Scene.Impl()
state_manager.register(StateManager.SCENE, scene)

state_manager.begin()

def on_update(dt):
    started = pyglet.clock._time.time()
    processed = 0
    for tid, seq, evt in server.recv():
        state_manager.dispatch_event(evt.event, tid, evt)
        processed += 1
    if processed > 0: debug("processed {} events in {}".format(processed,started-pyglet.clock._time.time()))
pyglet.clock.schedule_interval(on_update, 1/20.0)

if __name__ == "__main__": 
    server = Server()
    thread = threading.Thread(target=Server.accept, args=[server])
    thread.daemon = True
    thread.start()
    pyglet.app.run()
