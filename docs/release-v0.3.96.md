# v0.3.96

The native inference bridge stops scheduling judge retries when the guest
process exits. It still waits for an in-flight request and its guest
ledger acknowledgment. The native exit fixture verifies the recorded
HTTP status and cost for both a successful response and a rate-limited
response, with one start and one result in each run.

This covers process exit while the guest OS is still available. A guest
or storage failure during the receipt write can still leave a call
without a durable result.
