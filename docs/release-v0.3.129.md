# v0.3.129

The native pair runner evaluates the verified score with the pinned Agel
executable and benchmark library in a fresh Agel OS guest. It saves the
result and guest log beside the score source.

A replay of the retained adamant platebody pair produced correct outcomes
for both arms. The fixed arm took 27.299 seconds and the adaptive arm took
127.633 seconds, including its refused policy attempts. The adaptive arm
retained the fixed policy. Its two writer calls had unknown dollar costs,
so the native evaluator left the cost delta unknown.
