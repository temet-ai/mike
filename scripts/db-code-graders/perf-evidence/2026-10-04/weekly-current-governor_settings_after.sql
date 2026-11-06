SELECT key,value,updated_at FROM config.settings
          WHERE jsonb_typeof(value) IN ('number','boolean') AND key IN (
            'llm.rate_limit_per_minute','llm.ceiling_max_calls','llm.ceiling_window_secs',
            'llm.daily_spend_ceiling_usd','llm.spend_alert_fraction',
            'llm.budget.steerer.share_pct','llm.budget.session_judge.share_pct',
            'llm.budget.judge_worker.share_pct','llm.budget.lesson_relations.share_pct',
            'llm.budget.lesson_proposer.share_pct','llm.budget.eval.share_pct','llm.budget.other.share_pct',
            'judge_lessons.min_confidence','judge_lessons.max_per_day',
            'judge_lessons.auto_promote_min_evidence','judge_lessons.quiet_max_per_day',
            'judge_lessons.quiet_start_hour_utc','judge_lessons.quiet_end_hour_utc',
            'judge_lessons.quiet_idle_secs') ORDER BY key;
