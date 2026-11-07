# v0.3.152

The native scorer checks judge receipts in journal order. A result must
follow its matching start. Retry numbers must advance without gaps,
and a bridge dispatch must have a single judge identity.

The receipt fixture accepted an ordered retry and refused reordered,
overlapping, missing, and aliased receipts. The retained native quest
pair produced identical score input after the stricter check.
