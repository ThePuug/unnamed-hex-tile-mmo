"""What every build must satisfy before it is exported. These catch the
mistakes that are invisible in a render but break the client: a prop that
floats or sinks, one that spills past its tile, one that costs more
triangles than its budget. Facing and material are the critics' job."""

from mathutils import Vector

from . import mesh

## How far off the ground the lowest vertex may sit.
GROUND_TOLERANCE = 0.01


def bounds(objects):
    """World-space bounding box of every mesh as (min, max)."""
    lo = Vector((float("inf"),) * 3)
    hi = -lo
    for o in objects:
        if o.type != "MESH":
            continue
        for v in o.data.vertices:
            co = o.matrix_world @ v.co
            lo = Vector(map(min, lo, co))
            hi = Vector(map(max, hi, co))
    return lo, hi


def report(model, objects):
    """Stats and failures for one build: `dims`, `tris`, and `failures`, a
    list of one-line reasons, empty when the build passes."""
    lo, hi = bounds(objects)
    dims = hi - lo
    tris = sum(mesh.tris(o) for o in objects if o.type == "MESH")
    failures = []
    if abs(lo.z) > GROUND_TOLERANCE:
        failures.append(f"lowest point at z {lo.z:.3f}, not on the ground")
    centre = (lo + hi) / 2
    if abs(centre.x) > dims.x * 0.25 or abs(centre.y) > dims.y * 0.25:
        failures.append(f"centred at ({centre.x:.2f}, {centre.y:.2f}), not over the origin")
    for axis, got, box in zip("xyz", dims, model.box):
        if got > box + 1e-3:
            failures.append(f"{axis} extent {got:.2f} past the box's {box:.2f}")
    if tris > model.tris:
        failures.append(f"{tris} triangles past the budget of {model.tris}")
    return {"dims": tuple(dims), "tris": tris, "failures": failures}


def vertices(objects):
    """Every vertex of every mesh, for comparing two builds."""
    return [tuple(round(c, 5) for c in (o.matrix_world @ v.co))
            for o in objects if o.type == "MESH" for v in o.data.vertices]
