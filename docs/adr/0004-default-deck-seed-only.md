# Default Deck: seed on User create (not recreate on last delete)

From v1.1, deleting the last Deck does not recreate `Default`; an empty Deck list is allowed and the home UI shows create-Deck.

From v1.3, `Default` is seeded when each **User** is created (bootstrap admin and every admin-created User), not once per database file. Pre-v1.3 orphan Decks are assigned to the bootstrap admin instead of inventing a second global seed.
