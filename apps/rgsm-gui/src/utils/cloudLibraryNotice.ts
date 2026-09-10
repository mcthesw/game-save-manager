import type { CloudLibraryStatus } from '../api/generated/types.gen';

type SetupAction = 'inspect' | 'create' | 'reconnect' | 'rebuild';
export type CloudLibraryNotice = {
  messageKey: string;
  actionKey: string;
  action: SetupAction;
};

/** Presentation only: an empty location is not evidence that creation failed. */
export function cloudLibraryNotice(
  status: CloudLibraryStatus | null,
  inspectionFailed: boolean,
  initializationFailed: boolean
): CloudLibraryNotice | null {
  const notice = (message: string, actionKey: string, action: SetupAction): CloudLibraryNotice => ({
    messageKey: `sync_settings.library.${message}`,
    actionKey: `sync_settings.library.${actionKey}`,
    action,
  });
  if (inspectionFailed) return notice('inspect_failed', 'inspect', 'inspect');
  switch (status?.kind) {
    case 'empty':
      return initializationFailed
        ? notice('create_failed', 'retry_create', 'create')
        : notice('empty', 'create', 'create');
    case 'reconnect_required':
      return notice('reconnect_description', 'reconnect', 'reconnect');
    case 'rebuild_required':
      return notice('rebuild_description', 'rebuild', 'rebuild');
    default:
      return null;
  }
}
