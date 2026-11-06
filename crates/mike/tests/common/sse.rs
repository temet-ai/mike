pub fn sse_body(events: &[(&str, &str)]) -> String {
    let mut body = String::new();
    for (event_type, data) in events {
        body.push_str(&format!("event: {event_type}\ndata: {data}\n\n"));
    }
    body
}
