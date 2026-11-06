use mike::launch::fleet::test_seams::control::{ps_quiet, stat_group};

#[test]
fn process_names_and_zombie_rows_preserve_exact_group_identity() -> anyhow::Result<()> {
    assert_eq!(
        stat_group("123 (a (b) c) S 12 123 123 0").map_err(anyhow::Error::msg)?,
        ('S', 123)
    );
    assert!(stat_group("bad").is_err());
    assert!(stat_group("123 (name) S 1 nope").is_err());
    assert!(ps_quiet("123 123 Z\n124 123 Z+\n900 900 R", 123).map_err(anyhow::Error::msg)?);
    assert!(!ps_quiet("123 123 Z\n124 123 S", 123).map_err(anyhow::Error::msg)?);
    assert!(!ps_quiet("123 123 ?", 123).map_err(anyhow::Error::msg)?);
    assert!(ps_quiet("", 123).is_err());
    assert!(ps_quiet("123 nope Z", 123).is_err());
    assert!(ps_quiet("124 123 Z", 123).is_err());
    Ok(())
}
