import pyglet
from pyglet.window import key

PADDING = 8
BUTTON_SIZE = 48
NUM_ACTIONS = 8

class Button:
    __slots__ = ['slot', 'event']
    def __init__(self,slot,event):
        self.slot = slot
        self.event = event

class ActionBar(pyglet.event.EventDispatcher):
    def __init__(self,window,scene,batch):
        self.scene = scene
        self.bar = pyglet.shapes.Rectangle(window.width/2,0,PADDING+(BUTTON_SIZE+PADDING)*NUM_ACTIONS,2*PADDING+BUTTON_SIZE, color=(225, 225, 225, 100), batch=batch)
        self.bar.anchor_x = self.bar.width/2
        self.buttons = []
        for i in range(8):
            button = Button(pyglet.shapes.Rectangle(i*(PADDING+BUTTON_SIZE),0,BUTTON_SIZE,BUTTON_SIZE, color=(225, 225, 225, 100), batch=batch), None)
            button.slot.anchor_position = (self.bar.width/2 - window.width/2 - PADDING, -PADDING)
            self.buttons.append(button)
        self.buttons[0].event = ["on_action","on_overlay",None, self.scene.terrain]
        self.buttons[1].event = ["on_action","on_overlay",None, self.scene.streets]
        self.buttons[2].event = ["on_action","on_overlay",0, self.scene.decorators]
        self.buttons[3].event = ["on_action","on_overlay",None, self.scene.buildings]

    def on_key_press(self,sym,mod):
        if(sym == key.Q): self.dispatch_event(*self.buttons[0].event)
        if(sym == key.W): self.dispatch_event(*self.buttons[1].event)
        if(sym == key.E): self.dispatch_event(*self.buttons[2].event)
        if(sym == key.R): self.dispatch_event(*self.buttons[3].event)
        if(sym == key._1): self.dispatch_event(*self.buttons[4].event)
        if(sym == key._2): self.dispatch_event(*self.buttons[5].event)
        if(sym == key._3): self.dispatch_event(*self.buttons[6].event)
        if(sym == key._4): self.dispatch_event(*self.buttons[7].event)

ActionBar.register_event_type("on_action")