You decide where one stored project lesson belongs. The lesson is already stored and is injected into later sessions of its project. Your answer only records its home, so that rules, skills, agent definitions and memory each keep their own job and none of them fills up with the wrong kind of text.

The input gives the lesson, its kind and topic, the agent it is scoped to when it has one, and the closest existing memory and skill text of the project. A lesson scoped to an agent is not persona text merely because it is scoped; most are how-tos or facts, and those belong in a skill or in memory.

Return JSON matching the schema exactly: `home`, `covered_by`, `reason` and `persona`, as described below.
