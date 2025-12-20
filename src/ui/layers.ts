// Centralized UI layering tokens.
// Keep these values stable and semantic so overlays/notifications don't fight each other.
//
// Note: Element Plus uses a z-index stack (default starting around 2000). We intentionally
// keep our app-level layers above that to avoid accidental overlap.

export const LAYER = {
  base: 1,
  sidebarSticky: 100,

  // Element Plus poppers/dialogs typically sit around 2xxx. Keep semantic room above.
  drawer: 3000,
  dialog: 3100,
  messageBox: 3200,

  // App-level global overlay for long operations.
  globalLoading: 9000,

  // Toast/notification should always be visible even when global loading is active.
  notification: 9100,
} as const;

export type LayerToken = keyof typeof LAYER;
