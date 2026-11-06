UPDATE system_prompts.system_prompts SET last_seen_at = GREATEST(last_seen_at, now()) WHERE prompt_sha256 = '65c6a0a534091553900291de2fa6a6454843ee566a9796c82be1ca2ddd57dfdc'
