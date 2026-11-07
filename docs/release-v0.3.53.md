# v0.3.53 · Isolate a saved quest hand-in

Run `python3 scripts/test-native-network.py --quest --quest-resume EVIDENCE`
to reopen a native Cook's Assistant attempt that gathered all three
ingredients. The runner checks the world and earlier trace. The Agel
scenario then checks the account's inventory and Cooking XP and attempts
the dialogue. The run has its own evidence directory and checkpoint label.

On the private world, the resumed guest logged in and attempted to talk to
the cook three times. No dialogue opened. The server still held the three
ingredients and the quest remained at stage zero. A separate attempt that
stopped before gathering ingredients was refused as a checkpoint.
