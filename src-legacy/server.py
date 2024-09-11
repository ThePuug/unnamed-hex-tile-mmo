from collections import deque
import logging
from logging import debug, info, warning
import signal
import pyglet
import socket
import sys
import threading

from Actor.Actor import ImplFactory as ActorFactory
import Asset
import Behaviour
from Config import *
from Event import *
from LogId import LOGID
import Scene.Generator
import Scene.Scene
from Session import Session as Session
import StateManager

R = Scene.Scene.R*3

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
            sock.send(b'OK')
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

pyglet.resource.path = ['../../assets/sprites','../../data/maps']
pyglet.resource.reindex()

server = Server()
thread = threading.Thread(target=Server.accept, args=[server])
thread.daemon = True
thread.start()

behaviour_factory = Behaviour.Factory()
actor_factory = ActorFactory(behaviour_factory)
state_manager = StateManager.Impl(actor_factory)
server.push_handlers(state_manager)
state_manager.push_handlers(server)
scene = Scene.Scene.Impl(actor_factory, Asset.Factory(None), state_manager, Scene.Generator.Impl(42)) # TODO magic number
state_manager.register(StateManager.SCENE, scene)
state_manager.begin()

try:
    scene.tiles = scene.from_file()
except Exception as e:
    debug(e)
    state_manager.dispatch_event("on_try", None, TileDiscover((0,0,0)), None)

def on_update(dt):
    server.update(dt)
    for i,it in server.sessions.items():
        if it.do_exit.is_set():
            actor = state_manager.registry[StateManager.SCENE].pcs.get(i,None)
            if actor is None: warning("{:} - Actor not found: {}".format(LOGID.NF_ACTOR, i))
            else: state_manager.dispatch_event('on_do', None, ActorUnload(actor.state), True)
            it.sock.close()
            del server.sessions[i]
            break
    active = []
    inactive = list(scene.npcs.items())
    for i,it in list(scene.pcs.items()): 
        it.update(it.state,dt)
        for j,jt in list(inactive):
            if it.hx.dist(jt.hx) < R: 
                inactive.remove((j,jt))
                active.append((j,jt))
    for i,it in active:
        it.update(it.state,dt)
    if dt > 0.06: warning("dt > 0.06: {}".format(dt))
        
pyglet.clock.schedule_interval(on_update, 1/20.0)

if __name__ == "__main__": 
    signal.signal(signal.SIGINT, lambda sig,frame: state_manager.dispatch_event('on_close'))
    pyglet.app.run()
