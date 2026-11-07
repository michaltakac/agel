# v0.3.109

Run `python3 scripts/test-native-economy-pair.py --quest` for fixed and
live adaptive Cook's Assistant runs on fresh normal accounts. The
adaptive arm observes game packets during inference and includes
adaptation in its task clock. Both arms gather the ingredients.

The scorer verifies quest completion, quest points, Cooking XP, and
consumed ingredients against the saved server state. It can also score
a measured failure with no saved quest reward. Checkpoint hand-ins
remain a separate diagnostic.

Both installed clients and the scoring contract were checked with
fixture outcomes. The live pair is queued.
