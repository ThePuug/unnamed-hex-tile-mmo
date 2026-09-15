"""Scenes as build targets. One scene per seed; the export writes them all
into one GLB in name order, so a seed's scene is named by its index."""

import bpy


def fresh(name):
    """An empty scene called `name`, replacing any scene of that name."""
    old = bpy.data.scenes.get(name)
    if old is not None:
        clear(old)
        return old
    return bpy.data.scenes.new(name)


def clear(sc):
    for o in list(sc.collection.all_objects):
        data = o.data
        bpy.data.objects.remove(o)
        if data is not None and data.users == 0:
            for coll in (bpy.data.meshes, bpy.data.cameras, bpy.data.lights, bpy.data.curves):
                if data.name in coll and coll[data.name] == data:
                    coll.remove(data)
                    break


def only(names):
    """Removes every scene not in `names`, so the export holds the seeds
    asked for and nothing else."""
    for sc in list(bpy.data.scenes):
        if sc.name not in names:
            bpy.data.scenes.remove(sc)
