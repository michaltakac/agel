# v0.3.187

Native transfer reads its donor journal from the newest complete line
backwards and stops after finding the latest policy, workload, and profile
records. A hosted fixture covered duplicate records and a torn final line.

The preceding checksum change was also measured on a retained native
donor. Both recovery runs verified identical lineage checksums. Evaluator
steps fell from 8,766,661 to 7,091,421, and recovery took 46.4 seconds
versus 62.5 seconds in this pair.
