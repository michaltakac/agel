# v0.3.146

The bootstrap evaluator reuses owned values in `dict`, `get`, `assoc`,
and `dissoc`. These operations preserve map order and return the same
values while avoiding redundant deep copies of packet state.

Core and targeted library tests passed. The rebuilt Agel OS guest
reproduced the retained native quest score byte for byte. Native
gameplay timing remains unmeasured.
