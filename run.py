import logging
import pyglet
import sys
from Actor import Actor
from Console import Console
from logging import debug

LOGLEVEL = logging.DEBUG

logging.basicConfig(stream=sys.stderr, 
                    level=LOGLEVEL, 
                    format='%(levelname)-5s %(asctime)s %(module)s:%(funcName)s %(message)s',
                    datefmt="%Y-%m-%dT%H:%M:%S")
window = pyglet.window.Window(fullscreen=False)

batch_scene = pyglet.graphics.Batch()
img_scene = pyglet.image.SolidColorImagePattern((255,255,255,255)).create_image(window.width, window.height)

frames_blank = pyglet.image.ImageGrid(pyglet.resource.image("assets/sprites/blank.png"),rows=4,columns=4)
sprites = {
    "walk_n": pyglet.sprite.Sprite(pyglet.image.Animation.from_image_sequence([frames_blank[i] for i in [0,1,2,3]], duration=0.4)),
    "walk_w": pyglet.sprite.Sprite(pyglet.image.Animation.from_image_sequence([frames_blank[i] for i in [4,5,6,7]], duration=0.4)),
    "walk_e": pyglet.sprite.Sprite(pyglet.image.Animation.from_image_sequence([frames_blank[i] for i in [8,9,10,11]], duration=0.4)),
    "walk_s": pyglet.sprite.Sprite(pyglet.image.Animation.from_image_sequence([frames_blank[i] for i in [12,13,14,15]], duration=0.4))}
actor_blank = Actor(sprites)

batch_console = pyglet.graphics.Batch()
console = Console("",50,50,window.width-100,batch_console)

@console.event
def on_activate(obj=None):
    debug('args({})'.format(obj))
    if obj is not None: window.push_handlers(obj)

@console.event
def on_deactivate(obj=None):
    debug('args({})'.format(obj))
    if obj is not None: window.remove_handlers(obj)

@window.event
def on_key_press(sym,mod):
    debug('args({},{})'.format(sym,mod))
    if(sym == pyglet.window.key.QUOTELEFT and not console.enabled):
        console.dispatch_event('on_activate',console)

@window.event
def on_draw():
    window.clear()
    img_scene.blit(0,0)
    actor_blank.draw()
    if console.enabled: batch_console.draw()

if __name__ == "__main__": 
    pyglet.app.run()
