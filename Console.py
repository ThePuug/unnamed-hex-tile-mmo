import argparse
import cmd2
import pyglet
from logging import debug
from cmd2 import with_argparser
import console.CmdRecalculateRecipes

PADDING = 10

class Console(pyglet.event.EventDispatcher):
    def __init__(self, text, y, x, width, batch: pyglet.graphics.Batch):
        self.document = pyglet.text.document.UnformattedDocument(text)
        self.document.set_style(0, len(self.document.text), dict(color=(255, 255, 255, 255)))
        font = self.document.get_font()
        height = font.ascent - font.descent

        self.label = pyglet.text.Label('> ', font.name, font.size, x=x, y=y, anchor_x='center', anchor_y='bottom', color=(0,0,0,255), batch=batch)
        self.layout = pyglet.text.layout.IncrementalTextLayout(self.document, width, height, multiline=False, batch=batch)
        self.caret = pyglet.text.caret.Caret(self.layout)

        self.layout.x = x+PADDING
        self.layout.y = y
        self.border = pyglet.shapes.Rectangle(x-PADDING,y-PADDING,width+2*PADDING,height+2*PADDING, 
                                              color=(0, 0, 0, 50), batch=batch)

        self.enabled = False
        self.cmd = Cmd()

    def on_activate(self,obj):
        debug('args({})'.format(obj))
        if self is not obj: return
        self.enabled = True

    def on_key_press(self,sym,mod):
        debug('args({})'.format(sym,mod))
        if(sym == pyglet.window.key.TAB):
            completion = self.cmd.complete(self.document.text,0)
            if completion is not None: self.document.text = completion
            self.caret.position = len(self.document.text)
            return pyglet.event.EVENT_HANDLED
        elif(sym == pyglet.window.key.ENTER):
            self.enabled = False
            self.document.text=""
            self.cmd.runcmds_plus_hooks([self.document.text])
            self.dispatch_event('on_deactivate',self)
            return pyglet.event.EVENT_HANDLED
        elif(sym == pyglet.window.key.ESCAPE):
            self.enabled = False
            self.document.text=""
            self.dispatch_event('on_deactivate',self)
            return pyglet.event.EVENT_HANDLED
        
    def on_text(self,text): 
        debug('args({})'.format(text))
        if text != pyglet.window.key.QUOTELEFT: return self.caret.on_text(text)
    
    def on_text_motion(self,motion,select=False): 
        debug('args({},{})'.format(motion,select))
        return self.caret.on_text_motion(motion, select)
    
    def on_translation_update(self): 
        debug('args()'.format())
        return self.caret.on_translation_update()

Console.register_event_type('on_activate')
Console.register_event_type('on_deactivate')

class Cmd(cmd2.Cmd):
    def __init__(self, *args, **kwargs):
        super().__init__(allow_cli_args=False)
        del cmd2.Cmd.do_alias
        del cmd2.Cmd.do_run_pyscript
        del cmd2.Cmd.do_run_script
        del cmd2.Cmd.do_edit
        del cmd2.Cmd.do_set
        del cmd2.Cmd.do_shortcuts
        del cmd2.Cmd.do_shell

    parser_recalculate = cmd2.Cmd2ArgumentParser()
    parser_recalculate_subs = parser_recalculate.add_subparsers(title='category', help="category of data to recalculate")

    @with_argparser(parser_recalculate)
    def do_recalculate(self, ns: argparse.Namespace):
        handler = ns.cmd2_handler.get()
        if handler is None: self.do_help('recalculate')
        else: handler(ns)
