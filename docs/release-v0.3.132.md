# v0.3.132

The RuneScape quest advances when the empty pot reaches inventory.
The ground observation still identifies the pickup target and is reported
afterward. It can lag the inventory update or reflect a respawn.

The previous native adaptive run timed out waiting for the pickup even
though the server saved the pot on that account. The quest now tests its
gathering predicates after each decoded packet. Five interface tests passed,
including a fixture where a transient observation disappears before the
end of a tick window. The updated live outcome is pending.
