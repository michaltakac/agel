# v0.3.138

The guest bootstrap evaluator yields every 1,024 evaluation steps,
down from 4,096. Taking the tail of a list reuses the evaluator's
owned argument and avoids cloning every remaining element again.

Core and targeted library tests passed. A rebuilt Agel OS guest encoded
a 4,614-byte payload and exited successfully. A full transfer run with
the rebuilt evaluator is pending.
