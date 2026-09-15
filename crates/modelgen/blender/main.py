"""Command line inside Blender. Run through `cargo run --bin modelgen`,
which finds Blender and passes everything after `--` here."""

import argparse
import os
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import bpy  # noqa: E402

import modelgen  # noqa: E402
from modelgen import check, export, follow, models, proof, scene  # noqa: E402

ROOT = HERE.parents[2]

## Seeds in one asset, one glTF scene each.
VARIANTS = 3


def parse(argv):
    ap = argparse.ArgumentParser(prog="modelgen", description="Builds one model, or all of them, from a seed.")
    ap.add_argument("name", nargs="?", help="model name from --list, or `all`")
    ap.add_argument("--list", action="store_true", help="print every model with its brief and exit")
    ap.add_argument("--seed", type=int, default=0, help="first seed; the shipped asset starts at 0")
    ap.add_argument("--variants", type=int, default=VARIANTS,
                    help=f"seeds to build, one glTF scene each; the shipped asset holds {VARIANTS}")
    ap.add_argument("--size", type=int, default=512, help="proof sheet view size in pixels")
    # Absolute, because Blender resolves a relative render path against the
    # blend file, not the working directory.
    ap.add_argument("--out", type=absolute, default=ROOT / "assets/models", help="where the GLB goes")
    ap.add_argument("--proof", type=absolute, default=ROOT / "target/modelgen", help="where the proof sheets go")
    ap.add_argument("--test", action="store_true",
                    help="build every model twice at one seed, require identical meshes and passing checks; write nothing")
    return ap.parse_args(argv)


def absolute(s):
    return Path(s).resolve()


def fail(msg):
    print(f"modelgen: {msg}", file=sys.stderr)
    sys.exit(1)


def chosen(name):
    if name == "all":
        return list(models.MODELS)
    try:
        return [models.find(name)]
    except KeyError:
        fail(f"no model `{name}`; `--list` names them")


def test():
    ok = True
    for m in models.MODELS:
        a = modelgen.build(m, 7, scene.fresh("a"))
        b = modelgen.build(m, 7, scene.fresh("b"))
        if check.vertices(a) != check.vertices(b):
            print(f"{m.name:<16} differs between runs")
            ok = False
        for f in check.report(m, a)["failures"]:
            print(f"{m.name:<16} {f}")
            ok = False
        print(f"{m.name:<16} {'ok' if ok else 'FAIL'}")
    sys.exit(0 if ok else 1)


def main():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    args = parse(argv)
    bpy.ops.wm.read_factory_settings(use_empty=True)
    if args.list:
        for m in models.MODELS:
            print(f"{m.name:<16} {m.brief}")
        return
    if args.test:
        test()
    if args.name is None:
        fail("give a model name or `all`; `--list` names them")
    if args.variants < 1:
        fail("--variants must be at least 1")
    todo = chosen(args.name)
    for d in (args.out, args.proof):
        d.mkdir(parents=True, exist_ok=True)

    all_pass = True
    for m in todo:
        names = [str(k) for k in range(args.variants)]
        # The startup scene stays active, so the GLB's default scene is seed 0.
        bpy.context.scene.name = names[0]
        builds = []
        for k, name in enumerate(names):
            sc = scene.fresh(name)
            objects = modelgen.build(m, args.seed + k, sc)
            builds.append((sc, objects, check.report(m, objects), check.bounds(objects)))
        scene.only(names)
        asset = args.out / f"{m.name}.glb"
        export.glb(asset)
        for k, (sc, objects, rep, bounds) in enumerate(builds):
            sheet = args.proof / f"{m.name}-{k}.png"
            proof.sheet(sc, bounds, sheet, args.size)
            d = rep["dims"]
            verdict = "fits" if not rep["failures"] else "FAIL"
            all_pass &= not rep["failures"]
            print(f"{m.name:<16} seed {args.seed + k:<3} tris {rep['tris']:<5} "
                  f"{d[0]:.2f}x{d[1]:.2f}x{d[2]:.2f} {verdict:<5} {sheet}")
            for f in rep["failures"]:
                print(f"{'':<16} {f}")
        print(f"{m.name:<16} {args.variants} seeds -> {asset}")
        if follow.push(m.name, args.seed) is not None:
            print(f"{m.name:<16} seed {args.seed:<3} -> live Blender")
    sys.exit(0 if all_pass else 1)


main()
