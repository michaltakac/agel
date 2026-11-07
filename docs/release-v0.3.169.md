# v0.3.169

While waiting for the mining-site icon, the RuneScape quest example can
detect a freshly reopened tutor menu, select advice again, and retry
the page transition once. The original page and fresh menu have
separate open and input-event checks.

The interface fixture reproduces the reopened menu and verifies that
the retry reaches the icon without another talk. The melee fixture
also verifies the current first-encounter and confirmed stance behavior.
