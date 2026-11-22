use serde_json::Value;

use super::*;

pub(super) async fn scan_message_window(
    ctx: &ScanCtx<'_>,
    window: &[i64],
    acc: &mut ScanAcc,
) -> Result<(), FindTextFailure> {
    let params = ctx.params;
    acc.unseeded.extend_from_slice(window);
    let messages = ctx
        .handle
        .find_first_appearance_messages(
            ctx.session_db_id,
            window,
            params.needle,
            params.case_sensitive,
        )
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, session_id = %ctx.session_id, "find_text: message window query failed");
            find_text_error(StatusCode::INTERNAL_SERVER_ERROR, "lookup failed")
        })?;
    let matched = messages.iter().any(|message| {
        !crate::web_base::body_search::match_offsets(
            &message.text,
            params.needle,
            params.case_sensitive,
        )
        .is_empty()
    });
    if matched {
        seed_own_search_calls(ctx, acc).await?;
    }
    for message in messages {
        absorb_message(ctx, &message, acc);
    }
    Ok(())
}

/// Seeds the session's own search calls from every window scanned since the
/// last seed, only once a window has text to judge: a call's result can
/// appear in a later window than the call.
async fn seed_own_search_calls(
    ctx: &ScanCtx<'_>,
    acc: &mut ScanAcc,
) -> Result<(), FindTextFailure> {
    let params = ctx.params;
    let window = std::mem::take(&mut acc.unseeded);
    let ids = ctx
        .handle
        .find_own_search_tool_use_ids(
            &window,
            params.needle,
            params.case_sensitive,
            &search_tool_suffixes(),
        )
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, session_id = %ctx.session_id, "find_text: own-search tool use lookup failed");
            find_text_error(StatusCode::INTERNAL_SERVER_ERROR, "lookup failed")
        })?;
    acc.own_calls.seed(ids);
    Ok(())
}

fn is_own_search_message(text: &str, params: &FindTextMatchParams<'_>, acc: &mut ScanAcc) -> bool {
    let Ok(message) = serde_json::from_str::<Value>(text) else {
        return false;
    };
    acc.own_calls
        .claims_message(&message, params.needle, params.case_sensitive)
}

fn absorb_message(
    ctx: &ScanCtx<'_>,
    message: &crate::store::persistence::rehydrate::FirstAppearanceMessage,
    acc: &mut ScanAcc,
) {
    let params = ctx.params;
    let offsets = crate::web_base::body_search::match_offsets(
        &message.text,
        params.needle,
        params.case_sensitive,
    );
    if offsets.is_empty() {
        return;
    }
    if is_own_search_message(&message.text, params, acc) {
        acc.own_skipped += offsets.len() as u64;
        return;
    }
    let spans = if params.include_reminders {
        Vec::new()
    } else {
        reminder_spans(&message.text)
    };
    for offset in offsets {
        if offset_in_spans(&spans, offset) {
            acc.reminder_skipped += 1;
            continue;
        }
        acc.full_count += 1;
        if acc.has_room(params.max_matches) {
            let found = crate::web_base::body_search::text_match_at(
                &message.text,
                offset,
                params.needle.len(),
                params.context_bytes,
            );
            acc.matches.push(unified_match(
                ctx,
                message.request_id as u64,
                Some(message.ord),
                found,
            ));
        }
    }
}
