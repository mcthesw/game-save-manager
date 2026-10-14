use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};
use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::quick_actions::{QuickActionManager, QuickActionType};
use rgsm_core::config::Config;

#[derive(Default)]
pub struct RegisteredHotkeys {
    bindings: Mutex<Vec<Binding>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Backup,
    Apply,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Binding {
    shortcut: Shortcut,
    action: Action,
}

fn bindings(config: &Config) -> Result<Vec<Binding>> {
    let mut bindings: Vec<Binding> = Vec::new();
    for (action, keys) in [
        (Action::Backup, &config.quick_action.hotkeys.backup),
        (Action::Apply, &config.quick_action.hotkeys.apply),
    ] {
        let keys = keys
            .iter()
            .filter(|key| !key.is_empty())
            .cloned()
            .collect::<Vec<_>>();
        if keys.is_empty() {
            continue;
        }
        let shortcut = Shortcut::try_from(keys.join("+"))
            .with_context(|| format!("Invalid hotkey: {}", keys.join("+")))?;
        if bindings.iter().any(|binding| binding.shortcut == shortcut) {
            bail!("Backup and restore cannot use the same hotkey");
        }
        bindings.push(Binding { shortcut, action });
    }
    Ok(bindings)
}

/// Restore the previous bindings if the OS rejects a replacement. Track each
/// successful OS operation so even a failed rollback leaves an accurate list.
fn replace_bindings(
    active: &mut Vec<Binding>,
    desired: &[Binding],
    mut register: impl FnMut(Binding) -> Result<()>,
    mut unregister: impl FnMut(Shortcut) -> Result<()>,
) -> Result<()> {
    if active == desired {
        return Ok(());
    }
    let previous = active.clone();
    let result = (|| {
        while let Some(binding) = active.last().copied() {
            unregister(binding.shortcut)?;
            active.pop();
        }
        for binding in desired {
            register(*binding)?;
            active.push(*binding);
        }
        Ok(())
    })();
    if let Err(error) = result {
        let mut recovery_errors = Vec::new();
        for binding in active.clone() {
            if previous.contains(&binding) {
                continue;
            }
            match unregister(binding.shortcut) {
                Ok(()) => active.retain(|entry| *entry != binding),
                Err(error) => recovery_errors.push(error.to_string()),
            }
        }
        for binding in previous {
            if active.contains(&binding) {
                continue;
            }
            match register(binding) {
                Ok(()) => active.push(binding),
                Err(error) => recovery_errors.push(error.to_string()),
            }
        }
        if !recovery_errors.is_empty() {
            bail!(
                "{error:#}; could not fully restore previous hotkeys: {}",
                recovery_errors.join("; ")
            );
        }
        return Err(error);
    }
    Ok(())
}

/// Validate before releasing working shortcuts. Startup logs a failure; saves
/// return it to the caller so a rejected shortcut is never reported as applied.
pub fn apply_hotkeys(
    app: &AppHandle,
    config: &Config,
    registered: &RegisteredHotkeys,
    manager: &Arc<QuickActionManager>,
) -> Result<()> {
    let desired = bindings(config)?;
    let mut active = registered
        .bindings
        .lock()
        .expect("registered hotkey list poisoned");
    replace_bindings(
        &mut active,
        &desired,
        |binding| {
            let manager = Arc::clone(manager);
            app.global_shortcut()
                .on_shortcut(binding.shortcut, move |_app, _shortcut, event| {
                    if event.state() != ShortcutState::Released {
                        return;
                    }
                    match binding.action {
                        Action::Backup => manager.trigger_backup(QuickActionType::Hotkey),
                        Action::Apply => manager.trigger_apply(QuickActionType::Hotkey),
                    }
                })
                .map_err(Into::into)
        },
        |shortcut| {
            app.global_shortcut()
                .unregister(shortcut)
                .map_err(Into::into)
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn invalid_or_duplicate_keys_are_rejected_before_registration() {
        let mut config = Config::default();
        assert!(bindings(&config).unwrap().is_empty());
        config.quick_action.hotkeys.backup = vec!["Ctrl".into(), "B".into(), "".into()];
        config.quick_action.hotkeys.apply = vec!["NotAKey".into()];
        assert!(bindings(&config).is_err());
        config.quick_action.hotkeys.apply = config.quick_action.hotkeys.backup.clone();
        assert!(bindings(&config).is_err());
    }

    #[test]
    fn occupied_replacement_restores_working_bindings() {
        let old = Binding {
            shortcut: Shortcut::try_from("Ctrl+B").unwrap(),
            action: Action::Backup,
        };
        let new = Binding {
            shortcut: Shortcut::try_from("Ctrl+N").unwrap(),
            action: Action::Backup,
        };
        let occupied = Binding {
            shortcut: Shortcut::try_from("Ctrl+O").unwrap(),
            action: Action::Apply,
        };
        let os = RefCell::new(vec![old]);
        let mut active = vec![old];
        let result = replace_bindings(
            &mut active,
            &[new, occupied],
            |binding| {
                if binding == occupied {
                    bail!("shortcut already taken");
                }
                os.borrow_mut().push(binding);
                Ok(())
            },
            |shortcut| {
                os.borrow_mut()
                    .retain(|binding| binding.shortcut != shortcut);
                Ok(())
            },
        );
        assert!(result.is_err());
        assert_eq!(active, vec![old]);
        assert_eq!(*os.borrow(), vec![old]);
    }
}
