# Default Deck: seed on first init only

From v1.1, `Default` is created only when the DB is first initialized. Deleting the last Deck no longer recreates `Default`; an empty Deck list is allowed and the home UI shows create-Deck. This removes a race-prone invariant and matches the product preference that the user can recreate Decks explicitly.
