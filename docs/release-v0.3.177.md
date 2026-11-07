# v0.3.177

The RuneScape bronze example uses fresh route completion to time the
furnace interaction. If the first interaction does not open the menu,
it approaches the furnace again before retrying. A delayed menu that
is already visible needs no second click.

The interface fixture verifies that both interactions follow observed
arrival and that the second approach can recover a missed menu.
