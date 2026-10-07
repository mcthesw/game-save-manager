use super::*;

#[test]
fn suggests_all_accounts_without_turning_real_globs_into_bindings() {
    let temp = temp_dir::TempDir::new().unwrap();
    for account in ["one", "two[old]"] {
        let dir = temp.path().join(account);
        std::fs::create_dir_all(&dir).unwrap();
        for slot in ["1.sav", "2.sav"] {
            std::fs::write(dir.join(slot), b"save").unwrap();
        }
    }
    let mut context = ResolutionContext::default();
    context
        .variables
        .insert("root".into(), temp.path().to_string_lossy().into_owned());
    let result = discover_variable(
        &["<var:root>/<var:account>/*.sav".into()],
        "account",
        &context,
    )
    .unwrap();
    assert_eq!(
        result
            .candidates
            .iter()
            .map(|c| c.value.as_str())
            .collect::<Vec<_>>(),
        vec!["one", "two[old]"]
    );
    assert_eq!(result.candidates[0].paths.len(), 2);
    assert!(!result.incomplete);
    assert!(!context.variables.contains_key("account"));
}

#[test]
fn unresolved_root_requires_configuration_without_guessing_another_devices_value() {
    let context = ResolutionContext::default();
    let result = discover_variable(
        &["<var:root>/<var:account>/*.sav".into()],
        "account",
        &context,
    )
    .unwrap();
    assert_eq!(result.missing_variables, vec!["root"]);
    let result = discover_variable(&["<var:root>/Game/*.sav".into()], "root", &context).unwrap();
    assert!(result.needs_root);
    assert!(result.candidates.is_empty());
}

#[test]
fn repeated_variable_must_have_the_same_value_and_missing_targets_are_not_suggestions() {
    let temp = temp_dir::TempDir::new().unwrap();
    for path in ["one/one/1.sav", "one/two/2.sav"] {
        let path = temp.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"save").unwrap();
    }
    let context = ResolutionContext::default();
    let pattern = format!(
        "{}/<var:account>/<var:account>/*.sav",
        temp.path().to_string_lossy().replace('\\', "/")
    );
    let result = discover_variable(&[pattern], "account", &context).unwrap();
    assert_eq!(result.candidates.len(), 1);
    assert_eq!(result.candidates[0].paths.len(), 1);
    let result = discover_variable(
        &[format!(
            "{}/missing/<var:account>/*.sav",
            temp.path().display()
        )],
        "account",
        &context,
    )
    .unwrap();
    assert!(result.candidates.is_empty());
}

#[test]
fn variables_after_globs_and_literal_roots_keep_their_roles() {
    let temp = temp_dir::TempDir::new().unwrap();
    let root = temp.path().join("saves[main]");
    for profile in ["profile1", "profile2"] {
        let path = root.join(profile).join("player").join("slot.sav");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"save").unwrap();
    }
    let mut context = ResolutionContext::default();
    context
        .variables
        .insert("root".into(), root.to_string_lossy().into_owned());
    let result = discover_variable(
        &["<var:root>/*/<var:account>/*.sav".into()],
        "account",
        &context,
    )
    .unwrap();
    assert_eq!(result.candidates.len(), 1);
    assert_eq!(result.candidates[0].value, "player");
    assert_eq!(result.candidates[0].paths.len(), 2);
}

#[test]
fn depth_limit_reports_incomplete_instead_of_claiming_no_matches() {
    let temp = temp_dir::TempDir::new().unwrap();
    let mut nested = temp.path().to_path_buf();
    for _ in 0..13 {
        nested.push("deep");
    }
    std::fs::create_dir_all(nested).unwrap();
    let mut context = ResolutionContext::default();
    context
        .variables
        .insert("root".into(), temp.path().to_string_lossy().into_owned());
    let result = discover_variable(
        &["<var:root>/**/<var:account>/*.sav".into()],
        "account",
        &context,
    )
    .unwrap();
    assert!(result.incomplete);
    assert!(result.candidates.is_empty());
}
