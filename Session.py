from collections import deque
from logging import debug, error
import pyglet
import quickle
import threading

from Config import *
from Event import *

registry = [ActorMoveEvent, ActorLoadEvent]
OK = b'\x4f\x4b'

class Session(pyglet.event.EventDispatcher):
    def __init__(self, sock, incoming, outgoing):
        self.sock = sock
        self.encoder = quickle.Encoder(registry=registry)
        self.decoder = quickle.Decoder(registry=registry)
        self.outgoing = outgoing
        self.incoming = incoming
        self.seq = -1

        thread = threading.Thread(target=Session.sync, args=[self])
        thread.daemon = True
        thread.start()
        self.tid = thread.ident

    def sync(self):
        rest = bytes()
        while True:
            while True:
                # read some more data
                try:
                    it = self.sock.recv(1024)
                except Exception as e:
                    error(e)
                    return False
                if not it: return False
                it = rest + it
                i = 0

                while len(it[i:]) > 0:
                    # OK is end of send
                    tok = it[i:i+2]
                    if tok == OK: break

                    # recv more if not enough available
                    sz = int.from_bytes(tok, 'big', signed=False)
                    if len(it[i:]) < sz: break
                    i += 2

                    # take an event
                    tok = it[i:i+sz]
                    i = i+sz
                    tid, seq, evt = self.decoder.loads(tok)
                    self.incoming.append((tid, seq, evt))
                if it[i:i+2] == OK: 
                    rest = it[i+2:]
                    break
                else: rest = it[i:]
            
            while self.outgoing:
                tid, seq, evt = self.outgoing.popleft()
                it = self.encoder.dumps((tid, seq, evt))
                try:
                    self.sock.send(len(it).to_bytes(2, 'big', signed=False))
                    self.sock.send(it)
                except Exception as e:
                    error(e)
                    return False
            self.sock.send(OK)

    def send(self, evt):
        self.seq += 1
        self.outgoing.append((self.tid, self.seq, evt))

    def recv(self):
        while self.incoming: yield self.incoming.popleft()

Session.register_event_type('do_load_actor')
Session.register_event_type('do_move_actor')
