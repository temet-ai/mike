-- memory.seat_memory reads a mail row only through an operator turn whose tag names its
-- message_id, and the refresh finds that turn through its own closed-turn mark, so inserting
-- mail never changes seat memory. The trigger bumped seat_memory_sources on every mail insert
-- and made concurrent mail inserts queue on that one row's lock until their transactions committed.
DROP TRIGGER IF EXISTS trg_mailbox_messages_seat_memory_generation ON mailbox.messages;
