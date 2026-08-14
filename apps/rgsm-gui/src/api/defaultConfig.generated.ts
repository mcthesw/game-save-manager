// This file is generated from Rust Config::default(). Do not edit.

import type { Config } from './generated/types.gen';

export const DEFAULT_CONFIG: Config = {
  version: '1.9.0',
  backup_path: 'save_data',
  games: [],
  settings: {
    prompt_when_not_described: false,
    extra_backup_when_apply: true,
    confirm_before_apply_latest: true,
    confirm_before_apply_snapshot: true,
    show_edit_button: false,
    prompt_when_auto_backup: true,
    exit_to_tray: true,
    cloud_settings: {
      auto_sync_interval: 0,
      root_path: '/game-save-manager',
      backend: {
        type: 'Disabled',
      },
      max_concurrency: 1,
    },
    locale: 'zh_SIMPLIFIED',
    default_delete_before_apply: false,
    default_expend_favorites_tree: false,
    home_page: '/',
    log_to_file: true,
    add_new_to_favorites: false,
    vn_scan_dirs: [],
    save_list_expand_behavior: 'always_closed',
    save_list_last_expanded: false,
    save_list_sort_mode: 'saved_order',
    save_list_sort_direction: 'asc',
    max_auto_backup_count: 0,
    max_extra_backup_count: 5,
    appearance: {
      custom_font_enabled: false,
      ui_font_family: '',
    },
    compression_preset: 'Standard',
    compute_archive_hash: false,
    verify_archive_before_apply: false,
  },
  favorites: [],
  quick_action: {
    quick_action_game_id: null,
    hotkeys: {
      apply: ['', '', ''],
      backup: ['', '', ''],
    },
    enable_sound: true,
    enable_notification: true,
    notify_when_unchanged: true,
    sounds: {
      success: {
        kind: 'default',
      },
      failure: {
        kind: 'default',
      },
    },
    game_automations: [],
  },
  devices: {},
};
