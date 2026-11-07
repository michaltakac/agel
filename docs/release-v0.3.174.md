# v0.3.174

Hosted RuneScape quest runs send their Agel phase observations to the
test console as they happen. The launcher also updates the existing
viewer status file when it is available. The quest report and its
independent saved-state checks still determine the result.

A hosted fixture verified that text and structured observations reach
the progress callback while the final value remains available for scoring.
