SELECT EXISTS (SELECT 1 FROM wright.reply_jobs WHERE state IN ('queued', 'running'))
