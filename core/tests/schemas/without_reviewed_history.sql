-- Only for tests constructing a pre-v28 database from a disposable current fixture.
DROP TRIGGER IF EXISTS changes_history_card_add;
DROP TRIGGER IF EXISTS changes_history_card_remove;
DROP TABLE IF EXISTS changes_history_heads;
DROP TABLE IF EXISTS changes_history_cards;
