# Copyright 2026 Jordy van Gils
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

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
