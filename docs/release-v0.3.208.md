# v0.3.208

The melee trainer can inspect up to 512 queued packets while waiting
for a combat stance update. The trainer still requires confirmation
before attacking under the new stance.

In a live run on the earlier build, the server saved Strength stance
while the guest exhausted its confirmation window. The larger window
addresses delayed updates in queued observations. An interface fixture
verified confirmation beyond the former 128-packet limit.
