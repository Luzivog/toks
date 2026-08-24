use super::{navigation::PageNavigation, Page};

#[test]
fn a_new_visit_replaces_forward_history_and_repeated_visits_are_ignored() {
    let mut navigation = PageNavigation::default();
    assert!(navigation.visit(Page::Rotation));
    assert!(navigation.back());
    assert_eq!(navigation.current(), Page::Overview);
    assert!(navigation.visit(Page::Daily));
    assert!(!navigation.visit(Page::Daily));
    assert!(!navigation.forward());
    assert!(navigation.back());
    assert_eq!(navigation.current(), Page::Overview);
}
