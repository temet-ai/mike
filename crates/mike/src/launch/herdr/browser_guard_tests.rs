use super::cases;

#[tokio::test]
async fn activation_waits_for_exact_receipt_and_submits_only_once() -> anyhow::Result<()> {
    cases::activation_waits_for_exact_receipt_and_submits_only_once().await
}

#[tokio::test]
async fn missing_or_mismatched_activation_refuses_without_starting_an_agent() -> anyhow::Result<()>
{
    cases::missing_or_mismatched_activation_refuses_without_starting_an_agent().await
}

#[tokio::test]
async fn busy_prompt_times_out_without_submitting_activation() -> anyhow::Result<()> {
    cases::busy_prompt_times_out_without_submitting_activation().await
}

#[tokio::test]
async fn ordinary_launches_never_call_the_backend_for_browser_setup() -> anyhow::Result<()> {
    cases::ordinary_launches_never_call_the_backend_for_browser_setup().await
}

#[test]
fn receipt_io_failures_and_nonmatching_content_are_not_success() -> anyhow::Result<()> {
    cases::receipt_io_failures_and_nonmatching_content_are_not_success()
}
