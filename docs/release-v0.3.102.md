# v0.3.102

The native inference bridge appends and syncs each Agel judge record to
a host recovery file before sending it to the guest ledger. Verdict
delivery still requires guest acknowledgment. The scorer checks the
recovery file against the guest ledger when both are available.

A QEMU fixture shut down the guest after dispatch and before the judge
replied. The guest retained the start, the recovery file retained the
result and cost, and the bridge failed closed. Recovery records are
available for inspection and are not automatically imported into Agel OS.
