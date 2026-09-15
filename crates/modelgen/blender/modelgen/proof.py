"""Proof sheet for the critics: four views of one seed in a 2x2 grid, with
the model standing on a hex of the tile's radius for scale. Top left is the
front (the +Y face), top right the left side, bottom left the back, bottom
right the game's own angle, pitched down as the client camera is."""

import math
import os

import bmesh
import bpy
import numpy as np
from mathutils import Matrix, Vector

from . import mesh

## Client camera pitch below horizontal: `camera_height / CAMERA_DISTANCE`
## from `common::camera`, atan(tan(35 degrees)).
GAME_PITCH = math.radians(35)
## Turnaround views look down just enough for the ground to hide the base
## cut, as terrain does in the game.
SIDE_PITCH = math.radians(8)
GROUND = mesh.srgb(0.36, 0.36, 0.34)
HEX = mesh.srgb(0.52, 0.52, 0.48)
SKY = mesh.srgb(0.62, 0.66, 0.70)


def rig(sc):
    """Adds the camera, ground, hex plate, and sky a sheet needs to `sc`."""
    bm = bmesh.new()
    bmesh.ops.create_circle(bm, cap_ends=True, radius=1.0, segments=6)
    # Flat-top: a vertex on +X, as the client's hex grid has it.
    bmesh.ops.rotate(bm, verts=bm.verts, cent=(0, 0, 0), matrix=Matrix.Rotation(-math.pi / 6, 3, "Z"))
    for v in bm.verts:
        v.co.z = -0.002
    hexplate = mesh.object("proof-hex", bm, [mesh.material("proof-hex", HEX)])
    bm = bmesh.new()
    # Small enough that the turnaround views keep sky above its far edge.
    bmesh.ops.create_grid(bm, x_segments=1, y_segments=1, size=3.0)
    for v in bm.verts:
        v.co.z = -0.004
    ground = mesh.object("proof-ground", bm, [mesh.material("proof-ground", GROUND)])
    cam = bpy.data.objects.new("proof-cam", bpy.data.cameras.new("proof-cam"))
    for o in (hexplate, ground, cam):
        sc.collection.objects.link(o)
    sc.camera = cam
    sc.world = bpy.data.worlds.get("proof-sky") or bpy.data.worlds.new("proof-sky")
    sc.world.color = SKY
    sc.render.engine = "BLENDER_WORKBENCH"
    sh = sc.display.shading
    sh.light = "STUDIO"
    sh.color_type = "MATERIAL"
    sh.show_shadows = True
    sc.render.film_transparent = False
    sc.view_settings.view_transform = "Standard"
    return cam


def sheet(sc, bounds, path, size):
    """Renders the four views of `sc` and writes them as one PNG at `path`,
    each view `size` pixels square."""
    lo, hi = bounds
    span = max(hi - lo)
    centre = Vector((0, 0, (lo.z + hi.z) / 2))
    cam = rig(sc)
    sc.render.resolution_x = sc.render.resolution_y = size
    sc.render.resolution_percentage = 100
    sc.render.image_settings.file_format = "PNG"
    views = [
        ("front", Vector((0, math.cos(SIDE_PITCH), math.sin(SIDE_PITCH))), True),
        ("left", Vector((-math.cos(SIDE_PITCH), 0, math.sin(SIDE_PITCH))), True),
        ("back", Vector((0, -math.cos(SIDE_PITCH), math.sin(SIDE_PITCH))), True),
        ("game", Vector((math.cos(GAME_PITCH) * 0.7, -math.cos(GAME_PITCH) * 0.7, math.sin(GAME_PITCH))), False),
    ]
    tiles = []
    for name, direction, ortho in views:
        if ortho:
            cam.data.type = "ORTHO"
            cam.data.ortho_scale = span * 1.4
            cam.location = centre + direction * 20.0
        else:
            cam.data.type = "PERSP"
            cam.data.lens = 50.0
            cam.location = centre + direction.normalized() * span * 3.2
        cam.rotation_euler = (centre - cam.location).to_track_quat("-Z", "Y").to_euler()
        sc.render.filepath = f"{path}.{name}.png"
        bpy.ops.render.render(write_still=True, scene=sc.name)
        tiles.append(_pixels(sc.render.filepath))
    # Image rows run bottom-up, so the bottom pair comes first.
    grid = np.concatenate([np.concatenate([tiles[2], tiles[3]], axis=1),
                           np.concatenate([tiles[0], tiles[1]], axis=1)], axis=0)
    out = bpy.data.images.new("proof-sheet", size * 2, size * 2, alpha=True)
    out.pixels.foreach_set(grid.ravel())
    out.filepath_raw = str(path)
    out.file_format = "PNG"
    out.save()
    bpy.data.images.remove(out)
    for name, _, _ in views:
        os.remove(f"{path}.{name}.png")


def _pixels(filepath):
    img = bpy.data.images.load(filepath)
    w, h = img.size
    buf = np.empty(w * h * 4, dtype=np.float32)
    img.pixels.foreach_get(buf)
    bpy.data.images.remove(img)
    return buf.reshape(h, w, 4)
