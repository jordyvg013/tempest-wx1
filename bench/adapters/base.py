"""Common benchmark adapter base protocol for WX-1 cross-format suite."""

from __future__ import annotations

from abc import ABC, abstractmethod
from pathlib import Path
from typing import Any, Mapping
import numpy as np
import xarray as xr


class BenchmarkAdapter(ABC):
    """Abstract base class for benchmark storage format adapters."""

    def __init__(self, format_id: str, label: str, comparison_class: str):
        self.format_id = format_id
        self.label = label
        self.comparison_class = comparison_class  # 'lossless' or 'bounded_error'
        self.opened_handle = None

    @abstractmethod
    def write(self, dataset: xr.Dataset, destination: Path, settings: Mapping[str, Any] | None = None) -> Path:
        """Write the given xarray Dataset to destination path and return artifact path."""
        pass

    @abstractmethod
    def open(self, path: Path) -> Any:
        """Open dataset at path and retain handle."""
        pass

    @abstractmethod
    def read_field(self, var_name: str) -> np.ndarray:
        """Read 2D field array from open handle."""
        pass

    @abstractmethod
    def close(self) -> None:
        """Close opened handle and release resources."""
        pass

    def sync_mode(self) -> str:
        """Documented sync/durability mode ('buffered' or 'durable')."""
        return "buffered"
