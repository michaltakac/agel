# v0.3.176

The RuneScape melee agent observes a player tick after sending its
stance reset before clicking the requested stance again. The retry
still requires the server's requested mode before training starts.

The interface fixture verifies that both stance retries wait for a
tick and that a delayed confirmation can succeed without retrying.
