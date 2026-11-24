# GNOME Shell test harness

Starts a private, headless GNOME Shell and drives the real extension, engine and Studio
without touching your desktop. See [docs/TESTING.md](../../docs/TESTING.md) for how to run it.

- `run.sh`: starts one isolated session and runs a driver script
- `engine_check.py`: the scenarios (`SPIKE_SCENARIO`)
- `studio_check.py`: clicks through Studio with real pointer input
- `driver.py`, `overlay.js`, `target.js`: the original overlay feasibility checks
- `extension/`: a test-only helper extension for window state, input and screenshots
- `full-check.sh`: every scenario in a row (about 30 minutes)
- `summarize.py`: prints a results file
