# v0.3.43 · Retry Cook’s Assistant dialogue

The native quest runner gathers the ingredients on a fresh account
and checks the saved hand-ins, quest point, and Cooking XP. In its first
live run, the server saved the egg, milk, and flour. The dialogue timed
out before the first quest stage was saved.

The updated Agel policy tries talking to the cook up to three times,
checking the observed dialogue after each attempt. Once a page arrives,
it logs the interface and follows the bounded dialogue route. A fixture
with a missing first page confirmed that the retry reaches both choices
and the reward. The fresh native retry remains pending.

Run `sh scripts/test-native-network.sh --quest`.

[First native quest attempt](evidence/v0.3.43-native-quest-first-attempt.txt)
