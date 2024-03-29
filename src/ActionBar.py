import pyglet
from pyglet.window import key

from HxPx import Hx

PADDING = 8
BUTTON_SIZE = 48
NUM_ACTIONS = 8

KEYS = [key.Q,key.W,key.E,key.R,key._1,key._2,key._3,key._4]

class Button:
    __slots__ = ['slot', 'event','key']
    def __init__(self,slot,event,key):
        self.slot = slot
        self.event = event
        self.key = key

class ActionBar(pyglet.event.EventDispatcher):
    def __init__(self,window,scene,batch):
        self.scene = scene
        self.bar = pyglet.shapes.Rectangle(window.width/2,0,PADDING+(BUTTON_SIZE+PADDING)*NUM_ACTIONS,2*PADDING+BUTTON_SIZE, color=(225, 225, 225, 100), batch=batch)
        self.bar.anchor_x = self.bar.width/2
        self.buttons = []
        for i in range(8):
            button = Button(pyglet.shapes.Rectangle(i*(PADDING+BUTTON_SIZE),0,BUTTON_SIZE,BUTTON_SIZE, color=(225, 225, 225, 100), batch=batch), 
                            None, 
                            pyglet.text.Label(str(key.symbol_string(KEYS[i])), bold=True, batch=batch))
            button.key.position = (window.width/2 - self.bar.width/2 + i*(PADDING+BUTTON_SIZE) + BUTTON_SIZE/2, PADDING+BUTTON_SIZE/2, 0)
            button.slot.anchor_position = (self.bar.width/2 - window.width/2 - PADDING, -PADDING)
            self.buttons.append(button)
        self.buttons[0].event = ["on_action","on_overlay",Hx(0,0,0),[("biomes",i) for i in range(1,len(self.scene.asset_factory._assets["biomes"]))]]
        self.buttons[2].event = ["on_action","on_overlay",Hx(0,0,1),[("decorators",i) for i in range(len(self.scene.asset_factory._assets["decorators"]))]]
        self.buttons[3].event = ["on_action","on_overlay",Hx(0,0,1),[("buildings",i) for i in range(len(self.scene.asset_factory._assets["buildings"]))]]

    def on_key_press(self,sym,mod):
        if sym == key.Q: self.dispatch_event(*self.buttons[0].event)
        if sym == key.W: self.dispatch_event(*self.buttons[1].event)
        if sym == key.E: self.dispatch_event(*self.buttons[2].event)
        if sym == key.R: self.dispatch_event(*self.buttons[3].event)
        if sym == key._1: self.dispatch_event(*self.buttons[4].event)
        if sym == key._2: self.dispatch_event(*self.buttons[5].event)
        if sym == key._3: self.dispatch_event(*self.buttons[6].event)
        if sym == key._4: self.dispatch_event(*self.buttons[7].event)

ActionBar.register_event_type("on_action")