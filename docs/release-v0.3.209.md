# v0.3.209

If the stance variable is still absent after a bounded retry, melee
training can proceed using XP in the requested skill as confirmation.
A contradictory observed stance still stops training. The comparison
still requires the final saved stance and XP to agree with the task.

The interface fixture withheld stance updates for both Strength and
Defence while simulating server-side stance changes. The trainer
completed both phases using observed XP. The live case remains to be
verified.
