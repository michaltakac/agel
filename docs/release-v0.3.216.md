# v0.3.216

The OSRS example adds Agel banking actions for an explicitly supplied
booth and interface layout. Opening a bank requires a new interface
open and fresh bank inventory data. Deposits and withdrawals of one,
five, or ten items verify both inventory changes.

A hosted fixture covered stale stock, a withdrawal, a deposit, and
insufficient stock. The protocol fixture verified that full and partial
inventory packets advance the inventory's observation count. The live
bank layout and an end-to-end crafting route remain to be validated.
