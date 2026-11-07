# v0.3.90 · Score optimization separately from gameplay

`python3 scripts/compare-native-combat.py --optimizer RUN_DIRECTORY`
emits an Agel score for a resolved live optimizer decision. It verifies
the protected judge journal against bridge dispatches and keeps writer
usage separate from unknown writer dollar costs.

New runs log the full guest optimization clock through replay and
commit or refusal. Older runs use the bridge inference window and
label that narrower clock explicitly.

The scorer verified a committed live donor and a refused live cadence
run. The refused run recorded 81,564 nanodollars in judge charges and
two writer receipts. Its bridge inference window was 72.897 seconds.
