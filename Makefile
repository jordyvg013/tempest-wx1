# WX-1 Makefile

PYTHON ?= python

.PHONY: all test bench-data bench-test bench-run bench-report verify clean

all: test

bench-data:
	$(PYTHON) bench/datasets/synthetic.py

bench-test:
	PYTHONPATH=python:. $(PYTHON) -m pytest -q bench/tests

bench-run:
	PYTHONPATH=python:. $(PYTHON) -m bench.smoke_runner

bench-report:
	@if [ -f bench/results/report.md ]; then \
		cat bench/results/report.md; \
	else \
		echo "No benchmark report found. Run 'make bench-run' first."; \
	fi

test: bench-test
	PYTHONPATH=python $(PYTHON) -m pytest -q python/tests

verify: bench-data bench-test bench-run bench-report
