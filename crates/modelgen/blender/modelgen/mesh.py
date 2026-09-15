"""Mesh construction through bmesh and the data API, never operators, so a
build depends on nothing in the Blender context and runs the same headless,
in a live session, and inside the tests.

Colours are linear RGB, like texgen; `srgb` converts a colour as an artist
picks it."""

import bmesh
import bpy
from mathutils import Matrix, Vector


def srgb(r, g, b):
    """A colour as an artist picks it, in sRGB, converted to linear."""
    def lin(c):
        return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4
    return (lin(r), lin(g), lin(b))


def material(name, rgb, roughness=0.8):
    """A Principled material of one flat colour. The viewport colour is set
    too, which is what the proof renders show."""
    m = bpy.data.materials.get(name) or bpy.data.materials.new(name)
    m.use_nodes = True
    bsdf = m.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
    bsdf.inputs["Roughness"].default_value = roughness
    m.diffuse_color = (*rgb, 1.0)
    m.roughness = roughness
    return m


def bmesh_new():
    return bmesh.new()


def icosphere(subdivisions, radius=1.0):
    bm = bmesh.new()
    bmesh.ops.create_icosphere(bm, subdivisions=subdivisions, radius=radius)
    return bm


def scale(bm, x, y, z):
    bmesh.ops.scale(bm, vec=Vector((x, y, z)), verts=bm.verts)


def displace(bm, f, verts=None):
    """Moves every vertex, or each of `verts`, along its normal by `f(co)`.
    Normals are those of the mesh before the move, so displacement does not
    compound."""
    bm.normal_update()
    moves = [(v, v.normal * f(v.co.copy())) for v in (bm.verts if verts is None else verts)]
    for v, d in moves:
        v.co += d


def cleave(bm, co, normal):
    """Cuts away everything on the `normal` side of the plane through `co`
    and fills the cut with one flat face."""
    geom = bmesh.ops.bisect_plane(bm, geom=bm.verts[:] + bm.edges[:] + bm.faces[:],
                                  plane_co=co, plane_no=normal, clear_outer=True)
    edges = [e for e in geom["geom_cut"] if isinstance(e, bmesh.types.BMEdge)]
    bmesh.ops.holes_fill(bm, edges=edges)


def bury(bm, depth):
    """Cuts off everything under `depth` above the lowest point and rests
    the cut on z = 0, so the object sits into the ground rather than on it."""
    cut = min(v.co.z for v in bm.verts) + depth
    cleave(bm, (0, 0, cut), (0, 0, -1))
    for v in bm.verts:
        v.co.z -= cut


def flat(bm):
    for f in bm.faces:
        f.smooth = False


def object(name, bm, materials=()):
    """Turns `bm` into a mesh object, unlinked; the caller puts it in a scene.
    Frees `bm`."""
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    for m in materials:
        me.materials.append(m)
    return bpy.data.objects.new(name, me)


def tris(obj):
    return sum(len(p.vertices) - 2 for p in obj.data.polygons)


def shear(bm, x_per_z, y_per_z):
    """Leans the mesh: every vertex slides in x and y by its height."""
    for v in bm.verts:
        v.co.x += x_per_z * v.co.z
        v.co.y += y_per_z * v.co.z


def facet(bm, normal, distance):
    """Flattens everything past the plane `normal . co = distance` onto it,
    so a rounded form gets one flat face. The face keeps its triangles,
    invisible under flat shading, so materials can still vary across it."""
    n = Vector(normal).normalized()
    for v in bm.verts:
        d = v.co.dot(n) - distance
        if d > 0:
            v.co -= n * d


def cone(bm, segments, radius_bottom, radius_top, height, z, material_index=0):
    """Adds a capped cone to `bm` standing on `z`, and returns its new
    vertices. A `radius_top` of 0 makes a point."""
    ret = bmesh.ops.create_cone(bm, cap_ends=True, cap_tris=False, segments=segments,
                                radius1=radius_bottom, radius2=radius_top, depth=height)
    bmesh.ops.translate(bm, verts=ret["verts"], vec=Vector((0, 0, z + height / 2)))
    for v in ret["verts"]:
        for f in v.link_faces:
            f.material_index = material_index
    return ret["verts"]


def sphere(bm, subdivisions, radius, centre, material_index=0):
    """Adds an icosphere to `bm` about `centre` and returns its new
    vertices."""
    ret = bmesh.ops.create_icosphere(bm, subdivisions=subdivisions, radius=radius)
    bmesh.ops.translate(bm, verts=ret["verts"], vec=Vector(centre))
    for v in ret["verts"]:
        for f in v.link_faces:
            f.material_index = material_index
    return ret["verts"]


def limb(bm, segments, radius_start, radius_end, start, end, material_index=0):
    """Adds a capped cone to `bm` running from `start` to `end` and returns
    its new vertices."""
    start, end = Vector(start), Vector(end)
    axis = end - start
    ret = bmesh.ops.create_cone(bm, cap_ends=True, cap_tris=False, segments=segments,
                                radius1=radius_start, radius2=radius_end, depth=axis.length)
    turn = Vector((0, 0, 1)).rotation_difference(axis).to_matrix()
    for v in ret["verts"]:
        v.co = turn @ (v.co + Vector((0, 0, axis.length / 2))) + start
        for f in v.link_faces:
            f.material_index = material_index
    return ret["verts"]
