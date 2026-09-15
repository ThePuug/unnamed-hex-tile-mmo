"""Model generator: each model is a module that builds one low-poly object
from a seed, in Blender. The module is the source; the GLB under
`assets/models` is its output, committed so the client runs without Blender.
An asset holds several seeds, one glTF scene each, so the client can pick
`Scene(k)` by world position and the same prop never repeats side by side.
Which models exist is the README; how one is made and reviewed is AGENTS.md.

Units: one Blender unit is one world unit, the hex radius. The ground is
z = 0 and a model's front faces +Y, which the glTF export turns into the
client's -Z, the heading `North`.
"""

import importlib
import sys

import bpy

from . import check, export, follow, mesh, models, noise, proof, scene


def build(model, seed, target):
    """Builds `model` at `seed` into scene `target` and returns its objects."""
    objects = model.build(models.Params(seed=seed))
    for o in objects:
        target.collection.objects.link(o)
    return objects


def reload():
    """Re-imports every module so a live session sees edits on disk."""
    for m in (noise, mesh, scene, check, proof, export, follow):
        importlib.reload(m)
    for name in [n for n in sys.modules if n.startswith(__name__ + ".models")]:
        importlib.reload(sys.modules[name])
    importlib.reload(models)


def live(name, seed=0):
    """Rebuilds `name` in the current scene of a running Blender, for
    exploring through the MCP. The scene is scratch: whatever is kept goes
    back into the module."""
    reload()
    model = models.find(name)
    target = bpy.context.scene
    scene.clear(target)
    objects = build(model, seed, target)
    frame()
    return check.report(model, objects)


def frame():
    """Fits every 3D viewport to the scene. Nothing to do headless."""
    wm = bpy.context.window_manager
    for window in getattr(wm, "windows", []):
        for area in window.screen.areas:
            if area.type != "VIEW_3D":
                continue
            region = next(r for r in area.regions if r.type == "WINDOW")
            with bpy.context.temp_override(window=window, area=area, region=region):
                bpy.ops.view3d.view_all()
