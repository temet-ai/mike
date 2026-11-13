//! The launch settings offer exactly the backends launch knows, in order.

use super::BackendId;

#[test]
fn backend_names_match_backend_id_all_in_order() {
    let from_enum: Vec<&str> = BackendId::ALL.iter().map(|id| id.as_str()).collect();
    assert_eq!(
        crate::settings::meta_launch::BACKEND_NAMES,
        from_enum.as_slice()
    );
}
