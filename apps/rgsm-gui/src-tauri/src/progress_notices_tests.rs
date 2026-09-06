use super::*;
use rgsm_core::cloud_sync::v2::RemoteProgressCandidate;

fn review(relation: ProgressRelation) -> V2ConflictReview {
    V2ConflictReview {
        game_id: "game".into(),
        manifest_revision: 1,
        local: None,
        requires_choice: false,
        candidates: vec![RemoteProgressCandidate {
            snapshot_id: "remote".into(),
            description: String::new(),
            created_at: None,
            device_id: None,
            devices: vec!["other".into()],
            relation,
            local_unique_snapshots: 0,
            remote_unique_snapshots: 1,
            common_ancestor: None,
            common_ancestor_created_at: None,
            local_available: false,
            cloud_available: true,
        }],
    }
}

#[test]
fn only_available_new_remote_progress_is_eligible() {
    for (relation, expected) in [
        (ProgressRelation::Same, false),
        (ProgressRelation::RemoteEarlier, false),
        (ProgressRelation::RemoteAhead, true),
        (ProgressRelation::DifferentProgress, true),
        (ProgressRelation::NoLocalPosition, true),
    ] {
        let mut book = ProgressNoticeBook::default();
        let mut review = review(relation);
        assert_eq!(book.observe("Game", "pc", &review), expected);
        review.candidates[0].cloud_available = false;
        assert!(!book.observe("Game", "pc", &review));
        assert!(book.pending().notices.is_empty());
        review.candidates[0].cloud_available = true;
        review.candidates[0].devices = vec!["pc".into()];
        assert!(!book.observe("Game", "pc", &review));
    }
}

#[test]
fn defer_survives_metadata_reads_but_not_new_progress() {
    let mut book = ProgressNoticeBook::default();
    book.retain_games("library", &BTreeSet::from(["game".into()]));
    let mut review = review(ProgressRelation::RemoteAhead);
    assert!(book.observe("Game", "pc", &review));
    let id = book.pending().notices[0].id.clone();
    book.defer(std::slice::from_ref(&id));
    review.manifest_revision += 1;
    review.candidates[0].description = "Updated description".into();
    assert!(!book.observe("Renamed", "pc", &review));
    assert!(book.pending().notices.is_empty());
    review.candidates[0].snapshot_id = "new-head".into();
    assert!(book.observe("Renamed", "pc", &review));
    book.defer(&[id]);
    assert_eq!(book.pending().notices.len(), 1);
    assert_eq!(book.pending().notices[0].game_name, "Renamed");
    assert!(!book.observe("Renamed", "pc", &review));
}

#[test]
fn library_switch_and_ineligible_games_clear_notices() {
    let mut book = ProgressNoticeBook::default();
    let games = BTreeSet::from(["game".into()]);
    book.retain_games("one", &games);
    book.observe("Game", "pc", &review(ProgressRelation::NoLocalPosition));
    book.retain_games("one", &games); // A failed read need not discard known progress.
    assert_eq!(book.pending().notices.len(), 1);
    book.retain_games("two", &games);
    assert!(book.pending().notices.is_empty());
    book.observe("Game", "pc", &review(ProgressRelation::NoLocalPosition));
    book.retain_games("two", &BTreeSet::new());
    assert!(book.pending().notices.is_empty());
}

#[test]
fn local_backups_do_not_repeat_a_deferred_remote_notice() {
    use rgsm_core::cloud_sync::v2::LocalProgressView;

    let mut book = ProgressNoticeBook::default();
    let mut review = review(ProgressRelation::DifferentProgress);
    assert!(book.observe("Game", "pc", &review));
    let old_id = book.pending().notices[0].id.clone();
    book.defer(std::slice::from_ref(&old_id));
    review.local = Some(LocalProgressView {
        snapshot_id: "new-local".into(),
        description: String::new(),
        created_at: None,
        device_id: None,
        local_available: true,
        cloud_available: true,
    });
    assert!(!book.observe("Game", "pc", &review));
    assert!(book.pending().notices.is_empty());
    review.candidates[0].relation = ProgressRelation::Same;
    assert!(!book.observe("Game", "pc", &review));
    assert!(book.pending().notices.is_empty());
}

#[test]
fn background_notification_does_not_spawn_a_window_or_run_in_http_only_mode() {
    assert!(should_notify(true, false, true));
    assert!(!should_notify(true, true, true));
    assert!(!should_notify(true, false, false));
    assert!(!should_notify(false, false, true));
}

#[test]
fn removing_a_candidate_or_an_unrelated_game_does_not_repeat_progress() {
    let mut book = ProgressNoticeBook::default();
    let mut review = review(ProgressRelation::DifferentProgress);
    let mut second = review.candidates[0].clone();
    second.snapshot_id = "second-branch".into();
    review.candidates.push(second);
    book.retain_games(
        "library",
        &BTreeSet::from(["game".into(), "other-game".into()]),
    );
    assert!(book.observe("Game", "pc", &review));
    let id = book.pending().notices[0].id.clone();
    book.defer(&[id]);
    review.candidates.reverse();
    assert!(!book.observe("Game", "pc", &review));
    review.candidates.pop();
    assert!(!book.observe("Game", "pc", &review));
    book.retain_games("library", &BTreeSet::from(["game".into()]));
    assert!(book.pending().notices.is_empty());
}
