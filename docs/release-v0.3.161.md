# v0.3.161

The OSRS training example uses its bounded retry and target selection
path from the first encounter. The first phase can finish on the
requested XP and a Hitpoints XP increase while the rat remains visible.
The standalone one-rat task still verifies removal.

The previous adaptive melee arm saved 8 Attack XP but exited while
waiting for removal. A simulated encounter verified that the revised
training reports the XP and records that the rat is still visible.
