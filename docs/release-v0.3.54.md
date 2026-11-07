# v0.3.54 · Retry an unconfirmed combat stance

The RuneScape example waits for the requested stance observation before
training the next skill. When the observation is missing, it selects the
other known stance, then retries the desired selection within a bounded
window. The native runner preserves observed values on failure.

The previous adaptive run reached 88 observed Attack XP, then timed out.
The server saved combat mode 1 and zero Strength and Defence XP. A
regression fixture now drops the first confirmation and checks that the
example reselects the stance before training resumes. A new matched
native run is needed to measure the effect on task completion.
