"""The GLB the client loads. Every scene in the file is exported, in name
order, so scene `k` is seed `k`."""

import bpy


def glb(path):
    bpy.ops.export_scene.gltf(
        filepath=str(path),
        export_format="GLB",
        use_active_scene=False,
        export_apply=True,
        export_yup=True,
        export_animations=True,
        export_cameras=False,
        export_lights=False,
    )
