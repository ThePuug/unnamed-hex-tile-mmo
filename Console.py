from logging import debug
import pyglet
from pyglet.window import key

import cmd2
import argparse
from cmd2 import with_argparser

import console.CmdRecalculateRecipes

PADDING = 10
MARGIN = 100

class Console(pyglet.event.EventDispatcher):
    def __init__(self, size, batch):
        self.document = pyglet.text.document.UnformattedDocument("")
        self.document.set_style(0, len(self.document.text), dict(color=(0, 0, 0, 255)))
        font = self.document.get_font()
        height = font.ascent - font.descent

        self.label = pyglet.text.Label('> ', font.name, font.size, x=MARGIN/2, y=MARGIN/2, anchor_x='center', anchor_y='bottom', color=(0,0,0,255), batch=batch)
        self.layout = pyglet.text.layout.IncrementalTextLayout(self.document, size.y-MARGIN, height, multiline=True, batch=batch)
        self.caret = pyglet.text.caret.Caret(self.layout,batch=batch)

        self.layout.x = MARGIN/2+PADDING
        self.layout.y = MARGIN/2
        self.border = pyglet.shapes.Rectangle(MARGIN/2-PADDING,MARGIN/2-PADDING,size.x-MARGIN+2*PADDING,height+2*PADDING, 
                                              color=(255, 255, 255, 100), batch=batch)

        self.cmd = Cmd()

    def toggle(self):
        self.document.text=""
        self.border.visible = not self.border.visible
        self.label.visible = not self.label.visible
        self.caret.visible = not self.caret.visible

    def on_key_press(self,sym,mod):
        debug("args({},{})".format(sym,mod))
        if(sym == pyglet.window.key.TAB):
            completion = self.cmd.complete(self.document.text,0)
            if completion is not None: self.document.text = completion
            self.caret.position = len(self.document.text)
        elif(sym == pyglet.window.key.ENTER):
            self.cmd.runcmds_plus_hooks([self.document.text])
            self.toggle()
        elif(sym == pyglet.window.key.ESCAPE):
            self.toggle()
        
    def on_text(self,text): 
        debug("args({})".format(text))
        if text != '`': return self.caret.on_text(text)
    
    def on_text_motion(self,motion,select=False): 
        return self.caret.on_text_motion(motion, select)
    
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
