# v0.3.46 · Live native test milestones

The Agel OS graphical console can follow a native game test running in a
headless guest. Pass the graphical console URL and the test's serial log
to the watcher.

```sh
python3 scripts/osrs/watch.py VIEWER_URL --native-log RUN_DIRECTORY/serial.log
```

The watcher reads the selected log and sends milestones to the console.
The guest exposes the latest text in `runescape-status`. Combat XP, quest
and economy progress, policy activation or fallback, and guest exit status
come from the trace. The benchmark runner verifies saved game outcomes
separately.
