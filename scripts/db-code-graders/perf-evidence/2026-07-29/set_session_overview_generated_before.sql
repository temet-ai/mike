UPDATE sessions.sessions
   SET overview = 'The operator asked for a bug fix.

The session fixed it and added tests.',
       overview_generated_at = '2026-07-29 19:08:16.578231+00'
 WHERE session_uuid = '4ac63015-c620-42ab-830c-3e109f0714aa';
