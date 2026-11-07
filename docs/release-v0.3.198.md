# v0.3.198

The Agel optimizer example checks whether adaptation is still needed
after each preparation observation and again before starting the
proposal agent. A completed task can leave preparation during replay
decoding or tape writing instead of waiting for the next phase.

The caller supplies the completion predicate. In the RuneScape example
it checks the combat observations, stops the temporary stream service,
and returns the latest game state. A 1 GiB native guest fixture verified
that a completion observed at the first preparation pulse exits before
replay decoding and inference.
