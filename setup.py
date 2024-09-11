import sys
from cx_Freeze import setup, Executable

# Dependencies are automatically detected, but it might need fine tuning.
build_exe_options = {
    "excludes": [],
    "packages": ["zoneinfo"],
    "include_files": ["assets", "data"],
}

# base="Win32GUI" should be used only for Windows GUI app
base = "Win32GUI" if sys.platform == "win32" else None

setup(
    name="run",
    version="0.1.0",
    description="Hex grid mmo (pre-alpha)",
    options={"build_exe": build_exe_options},
    executables=[Executable("src/client/run.py", base=base)],
)