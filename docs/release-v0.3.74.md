# v0.3.74 · Report writer usage coverage in melee scores

The evidence loader matches writer receipts to dispatch IDs and rejects
duplicate receipts, unknown job IDs, and impossible cache counts. Agel
sums the reported tokens and counts how many calls contributed to the
input, output, and cache totals. Missing receipts and absent cache
counts stay visible in the coverage report.

The historical melee pair still has two writer calls without token
receipts. It retains the measured judge costs and an unknown writer
dollar total. The native runner also logs the numeric login refusal code
when the server rejects an account.
