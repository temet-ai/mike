use super::*;

fn frame(event: &str, data: &str) -> Frame {
    Frame {
        raw: Bytes::from(format!("event: {event}\ndata: {data}\n\n")),
        event: event.to_owned(),
        data: data.to_owned(),
    }
}

fn message_start() -> Frame {
    frame(
        "message_start",
        r#"{"type":"message_start","message":{"id":"msg_1"}}"#,
    )
}

fn block_start(index: u64, kind: &str) -> Frame {
    frame(
        "content_block_start",
        &serde_json::json!({"type": "content_block_start", "index": index, "content_block": {"type": kind}})
            .to_string(),
    )
}

fn tool_start(index: u64, id: &str, name: &str) -> Frame {
    frame(
        "content_block_start",
        &serde_json::json!({
            "type": "content_block_start",
            "index": index,
            "content_block": {"type": "tool_use", "id": id, "name": name}
        })
        .to_string(),
    )
}

fn text(index: u64, text: &str) -> Frame {
    frame(
        "content_block_delta",
        &serde_json::json!({
            "type": "content_block_delta",
            "index": index,
            "delta": {"type": "text_delta", "text": text}
        })
        .to_string(),
    )
}

fn thinking(index: u64, thinking: &str) -> Frame {
    frame(
        "content_block_delta",
        &serde_json::json!({
            "type": "content_block_delta",
            "index": index,
            "delta": {"type": "thinking_delta", "thinking": thinking}
        })
        .to_string(),
    )
}

fn input_json(index: u64, partial: &str) -> Frame {
    frame(
        "content_block_delta",
        &serde_json::json!({
            "type": "content_block_delta",
            "index": index,
            "delta": {"type": "input_json_delta", "partial_json": partial}
        })
        .to_string(),
    )
}

fn block_stop(index: u64) -> Frame {
    frame(
        "content_block_stop",
        &serde_json::json!({"type": "content_block_stop", "index": index}).to_string(),
    )
}

fn forwarded(admit: Admit) -> anyhow::Result<Vec<Bytes>> {
    match admit {
        Admit::Forward(frames) => Ok(frames),
        Admit::Cut(_) => anyhow::bail!("expected the frame to be forwarded or held, got a cut"),
    }
}

fn words(list: &[&str]) -> Vec<String> {
    list.iter().map(|w| (*w).to_owned()).collect()
}

const ROSTER: &str = crate::proxy_tools::roster::TOOL_NAME;

#[test]
fn thinking_reaches_the_client_the_moment_it_arrives() -> anyhow::Result<()> {
    let mut gate = StreamGate::new(&words(&["forbidden-word"]), true);
    assert_eq!(forwarded(gate.admit(message_start()))?.len(), 1);
    assert_eq!(forwarded(gate.admit(block_start(0, "thinking")))?.len(), 1);
    let delta = thinking(0, "let me work this out");
    let raw = delta.raw.clone();
    assert_eq!(forwarded(gate.admit(delta))?, vec![raw]);
    Ok(())
}

#[test]
fn with_no_listed_phrase_text_is_not_held_at_all() -> anyhow::Result<()> {
    let mut gate = StreamGate::new(&[], true);
    forwarded(gate.admit(block_start(0, "text")))?;
    let delta = text(0, "Hello");
    let raw = delta.raw.clone();
    assert_eq!(forwarded(gate.admit(delta))?, vec![raw]);
    Ok(())
}

#[test]
fn text_is_released_as_soon_as_it_can_no_longer_be_the_start_of_a_listed_phrase()
-> anyhow::Result<()> {
    let mut gate = StreamGate::new(&words(&["abcd"]), false);
    forwarded(gate.admit(block_start(0, "text")))?;

    let first = text(0, "0123456789");
    let first_raw = first.raw.clone();
    assert!(
        forwarded(gate.admit(first))?.is_empty(),
        "the newest text is held: a phrase could still be starting in it"
    );

    let second = text(0, "xyz");
    assert_eq!(
        forwarded(gate.admit(second))?,
        vec![first_raw],
        "three newer characters cover the phrase's length less one, so the older text is safe"
    );
    Ok(())
}

#[test]
fn the_end_of_a_text_block_releases_what_was_held_before_it() -> anyhow::Result<()> {
    let mut gate = StreamGate::new(&words(&["forbidden-word"]), false);
    forwarded(gate.admit(block_start(0, "text")))?;
    let delta = text(0, "short");
    let delta_raw = delta.raw.clone();
    assert!(forwarded(gate.admit(delta))?.is_empty());

    let stop = block_stop(0);
    let stop_raw = stop.raw.clone();
    assert_eq!(forwarded(gate.admit(stop))?, vec![delta_raw, stop_raw]);
    Ok(())
}

#[test]
fn a_reply_that_ends_releases_its_held_tail() -> anyhow::Result<()> {
    let mut gate = StreamGate::new(&words(&["forbidden-word"]), false);
    forwarded(gate.admit(block_start(0, "text")))?;
    let delta = text(0, "tail");
    let raw = delta.raw.clone();
    assert!(forwarded(gate.admit(delta))?.is_empty());
    assert_eq!(gate.finish(), vec![raw]);
    Ok(())
}

#[test]
fn a_listed_phrase_cuts_the_reply_and_no_part_of_it_was_sent() -> anyhow::Result<()> {
    let mut gate = StreamGate::new(&words(&["pre-existing"]), false);
    let mut sent: Vec<Bytes> = Vec::new();
    sent.extend(forwarded(gate.admit(message_start()))?);
    sent.extend(forwarded(gate.admit(block_start(0, "text")))?);
    sent.extend(forwarded(
        gate.admit(text(0, "The failure is a long-standing one, ")),
    )?);
    sent.extend(forwarded(gate.admit(text(0, "and it is pre-")))?);

    let Admit::Cut(Cut::Word {
        word, text, kept, ..
    }) = gate.admit(self::text(0, "existing in main."))
    else {
        anyhow::bail!("the phrase completed across two deltas and must cut the reply");
    };
    assert_eq!(word, "pre-existing");
    assert_eq!(
        text,
        "The failure is a long-standing one, and it is pre-existing in main."
    );
    assert_eq!(kept, "The failure is a long-standing one, and it is ");

    let sent_text = String::from_utf8(sent.concat())?;
    assert!(
        !sent_text.contains("pre-"),
        "the start of the phrase was still held when the rest arrived; sent:\n{sent_text}"
    );
    assert_eq!(
        gate.stitch_point(),
        StitchPoint {
            open_block: Some(0),
            next_index: 1
        },
        "the client has a text block open that must be closed before more is sent"
    );
    Ok(())
}

fn delta_text(frame: &Bytes) -> anyhow::Result<String> {
    let raw = std::str::from_utf8(frame)?;
    let Some(data) = raw.lines().find_map(|l| l.strip_prefix("data: ")) else {
        return Ok(String::new());
    };
    let json: serde_json::Value = serde_json::from_str(data)?;
    if json
        .pointer("/delta/type")
        .and_then(serde_json::Value::as_str)
        != Some("text_delta")
    {
        return Ok(String::new());
    }
    Ok(json["delta"]["text"]
        .as_str()
        .unwrap_or_default()
        .to_owned())
}

fn client_text(frames: &[Bytes]) -> anyhow::Result<String> {
    frames.iter().map(delta_text).collect()
}

fn cut_on(gate: &mut StreamGate, frame: Frame) -> anyhow::Result<(String, Option<Bytes>)> {
    match gate.admit(frame) {
        Admit::Cut(Cut::Word {
            kept, kept_frame, ..
        }) => Ok((kept, kept_frame)),
        _ => anyhow::bail!("the phrase completed and must cut the reply"),
    }
}

#[test]
fn word_cut_split_across_two_deltas_client_ends_exactly_before_the_word() -> anyhow::Result<()> {
    let mut gate = StreamGate::new(&words(&["pre-existing"]), false);
    let mut sent: Vec<Bytes> = Vec::new();
    sent.extend(forwarded(gate.admit(message_start()))?);
    sent.extend(forwarded(gate.admit(block_start(0, "text")))?);
    sent.extend(forwarded(
        gate.admit(text(0, "The database migration fail")),
    )?);
    sent.extend(forwarded(gate.admit(text(0, "ed because of a pre-")))?);

    let (kept, kept_frame) = cut_on(&mut gate, text(0, "existing table."))?;
    sent.extend(kept_frame);

    assert_eq!(kept, "The database migration failed because of a ");
    assert_eq!(client_text(&sent)?, kept);
    for frame in &sent {
        let raw = String::from_utf8(frame.to_vec())?;
        assert!(!raw.contains("pre-"), "a frame carries the word: {raw}");
    }
    assert!(
        gate.finish().is_empty(),
        "the held frames are never released"
    );
    Ok(())
}

#[test]
fn word_cut_inside_one_long_delta_client_receives_the_part_before_it() -> anyhow::Result<()> {
    let mut gate = StreamGate::new(&words(&["pre-existing"]), false);
    let mut sent: Vec<Bytes> = Vec::new();
    sent.extend(forwarded(gate.admit(block_start(0, "text")))?);
    sent.extend(forwarded(gate.admit(text(0, "Checking the logs. ")))?);

    let (kept, kept_frame) = cut_on(
        &mut gate,
        text(
            0,
            "The database migration failed because of a Pre-Existing table.",
        ),
    )?;
    sent.extend(kept_frame);

    assert_eq!(
        kept,
        "Checking the logs. The database migration failed because of a "
    );
    assert_eq!(client_text(&sent)?, kept);
    let all = String::from_utf8(sent.concat())?;
    assert!(!all.to_lowercase().contains("pre-existing"), "{all}");
    Ok(())
}

#[test]
fn word_cut_at_the_start_of_a_delta_sends_no_extra_frame() -> anyhow::Result<()> {
    let mut gate = StreamGate::new(&words(&["robust"]), false);
    let mut sent: Vec<Bytes> = Vec::new();
    sent.extend(forwarded(gate.admit(block_start(0, "text")))?);
    sent.extend(forwarded(gate.admit(text(0, "A plan that is ")))?);
    sent.extend(gate.finish());

    let (kept, kept_frame) = cut_on(&mut gate, text(0, "robust."))?;

    assert_eq!(kept, "A plan that is ");
    assert!(kept_frame.is_none(), "everything before the word was sent");
    assert_eq!(client_text(&sent)?, kept);
    Ok(())
}

#[test]
fn a_roster_call_is_held_from_its_first_frame_and_answered_when_it_closes() -> anyhow::Result<()> {
    let mut gate = StreamGate::new(&[], true);
    forwarded(gate.admit(message_start()))?;
    forwarded(gate.admit(block_start(0, "text")))?;
    forwarded(gate.admit(text(0, "Checking.")))?;
    forwarded(gate.admit(block_stop(0)))?;

    assert!(
        forwarded(gate.admit(tool_start(1, "toolu_09", ROSTER)))?.is_empty(),
        "the client must never see a tool it does not have"
    );
    assert!(forwarded(gate.admit(input_json(1, "{}")))?.is_empty());

    let Admit::Cut(Cut::ToolCall { call, prelude }) = gate.admit(block_stop(1)) else {
        anyhow::bail!("closing the roster call must hand it over to be answered");
    };
    assert_eq!(call.id, "toolu_09");
    assert_eq!(call.name, ROSTER);
    assert_eq!(call.input_json, "{}");
    assert_eq!(
        prelude.text, "Checking.",
        "text before the call is part of the same assistant turn"
    );
    assert_eq!(
        gate.stitch_point(),
        StitchPoint {
            open_block: None,
            next_index: 1
        },
        "the answer's blocks continue after the one text block the client has"
    );
    Ok(())
}

#[test]
fn a_call_to_one_of_the_clients_own_tools_streams_through() -> anyhow::Result<()> {
    let mut gate = StreamGate::new(&[], true);
    assert_eq!(
        forwarded(gate.admit(tool_start(0, "toolu_10", "Read")))?.len(),
        1
    );
    assert_eq!(
        forwarded(gate.admit(input_json(0, r#"{"file_path":"/x"}"#)))?.len(),
        1
    );
    assert_eq!(forwarded(gate.admit(block_stop(0)))?.len(), 1);
    Ok(())
}

#[test]
fn a_gate_that_answers_no_tool_calls_lets_a_roster_call_through() -> anyhow::Result<()> {
    let mut gate = StreamGate::new(&words(&["forbidden-word"]), false);
    assert_eq!(
        forwarded(gate.admit(tool_start(0, "toolu_11", ROSTER)))?.len(),
        1
    );
    Ok(())
}

#[test]
fn frames_the_gate_does_not_know_pass_through_in_order_behind_held_text() -> anyhow::Result<()> {
    let mut gate = StreamGate::new(&words(&["forbidden-word"]), false);
    forwarded(gate.admit(block_start(0, "text")))?;
    let delta = text(0, "held");
    let delta_raw = delta.raw.clone();
    forwarded(gate.admit(delta))?;

    let ping = frame("ping", r#"{"type":"ping"}"#);
    let ping_raw = ping.raw.clone();
    assert_eq!(forwarded(gate.admit(ping))?, vec![delta_raw, ping_raw]);
    Ok(())
}
