from logging import debug, error
import threading

from Config import *
from Event import *
import ormsgpack

class Session():
    def __init__(self, sock, incoming, outgoing):
        self.sock = sock
        self.outgoing = outgoing
        self.incoming = incoming

        self.do_exit = threading.Event()
        self.thread = threading.Thread(target=Session.sync, args=[self])
        self.thread.daemon = True
        self.thread.start()

    def sync(self):

        rest = bytes()
        while True:
            while True:
                # read some more data
                try:
                    it = self.sock.recv(1024)
                except Exception as e:
                    error(e)
                    self.do_exit.set()
                    return False
                if not it: 
                    self.do_exit.set()
                    return False
                it = rest + it
                i = 0

                while len(it[i:]) > 2:
                    # OK is end of send
                    tok = it[i:i+2]
                    if tok == b'OK': break

                    # recv more if not enough available
                    sz = int.from_bytes(tok, 'big', signed=False)
                    if len(it[i+2:]) < sz: break
                    i += 2

                    # take an event
                    tok = it[i:i+sz]
                    i = i+sz
                    try:
                        tid, evt, seq = ormsgpack.unpackb(tok)
                    except Exception as e:
                        debug(e)
                        continue
                    self.incoming.append((tid, Event.from_dict(evt), seq))
                if it[i:i+2] == b'OK': 
                    rest = it[i+2:]
                    break
                else: rest = it[i:]

            while self.outgoing:
                tid, evt, seq = self.outgoing.popleft()
                it = ormsgpack.packb((tid, {type(evt).__name__: evt}, seq))
                try:
                    self.sock.send(len(it).to_bytes(2, 'big', signed=False))
                    self.sock.send(it)
                except Exception as e:
                    error(e)
                    return False
            self.sock.send(b'OK')

    def on_send(self, tid, evt, seq):
        self.outgoing.append((tid, evt, seq))

    def recv(self):
        while self.incoming: yield self.incoming.popleft()
