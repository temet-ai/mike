-- Convert steerer rows still pinned to the historical seeded default model
-- into the inherit sentinel (empty string), so a change to the configured
-- `steerer.model` default actually reaches them via fleet.rs's per-row
-- resolution (issue #125 AC5). The historical seed value is named exactly
-- once, here, so it never needs re-litigating elsewhere in the codebase.
UPDATE steering.steerers
   SET model = ''
 WHERE model = 'google/gemini-2.5-flash-lite';
