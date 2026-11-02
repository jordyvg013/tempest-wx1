from __future__ import annotations

import os
from pathlib import Path
import subprocess
import sys


def test_python_package_import_does_not_import_optional_or_gpu_stack():
    package_root = Path(__file__).resolve().parents[1]
    script = r'''
import importlib.abc
import sys

blocked_roots = {"numpy", "xarray", "dask", "jax", "cupy", "torch"}
class BlockOptional(importlib.abc.MetaPathFinder):
    def find_spec(self, fullname, path=None, target=None):
        root = fullname.split(".", 1)[0]
        if root in blocked_roots:
            raise ModuleNotFoundError(f"blocked optional dependency: {root}")
        return None

sys.meta_path.insert(0, BlockOptional())
import wx1
assert "numpy" not in sys.modules
assert "xarray" not in sys.modules
assert "dask" not in sys.modules
assert "jax" not in sys.modules
assert "cupy" not in sys.modules
assert "torch" not in sys.modules
print("OK: wx1 package imported without optional scientific or GPU modules")
'''
    env = os.environ.copy()
    env["PYTHONPATH"] = str(package_root)
    result = subprocess.run(
        [sys.executable, "-S", "-c", script],
        env=env,
        text=True,
        capture_output=True,
        check=False,
    )
    assert result.returncode == 0, result.stdout + result.stderr
    assert "OK: wx1 package imported" in result.stdout
