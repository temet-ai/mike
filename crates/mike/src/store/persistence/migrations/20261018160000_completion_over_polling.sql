UPDATE steering.steerers
   SET description = 'Detects foreground waits for background work and recommends completion notifications or fleet_check completion mail.',
       pattern = E'sleep \nsleep(\nuntil grep\nwhile true\ntask-wait',
       prompt = $prompt$You judge whether an assistant turn waits in the foreground for work whose completion can be delivered asynchronously.

Look at the actual Tool calls this turn. Fire for a foreground sleep of 30 seconds or more waiting for a build, test, deploy, CI or another session; a loop that sleeps and checks a process/file/condition; or scripts/task-wait polling output that is still pending. task-wait is itself a polling loop, even without a literal sleep in the tool call.

Do not fire for a finished output read with task-wait --grade-only; an immediate status read with --timeout 0; a wait already running in the background; a short settle pause followed by one read; an explicitly requested measurement window or rate-limit delay; a command actually doing work; or quoted examples. Do not treat starting a background task as a successful result.

When firing, recommend using the existing task's completion notification and doing independent work, or ending the turn while waiting for that notification. In a fleet that exposes fleet_check, recommend queueing future narrow checks with fleet_check(command) and receiving completion mail. For conditions without a completion event, recommend Monitor only when that tool is available in the session. Do not recommend scripts/task-wait as a waiting mechanism or assume every session has Monitor. After completion, read the actual command result once; inside temet-proxy, scripts/task-wait <output> --grade-only reads in-band verdicts without polling.

Be conservative: when the command is not clearly waiting for other work, do not fire.

Return JSON: fires (bool), confidence (0.0-1.0), reason (why), nudge (one short instruction naming the foreground wait and the applicable completion mechanism when fires=true; empty otherwise).$prompt$,
       updated_at = NOW()
 WHERE name = 'monitor-over-sleep'
   AND deleted_at IS NULL;
