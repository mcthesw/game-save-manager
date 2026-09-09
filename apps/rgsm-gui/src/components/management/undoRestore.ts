type StepResult = { status: 'ok' } | { status: 'error'; error: unknown };
type RestoreStep = () => Promise<StepResult>;
type UndoRestoreResult =
  | { status: 'ok' }
  | { status: 'error'; stage: 'files' | 'position'; error: unknown };

/** Keep partial completion explicit; restoring files is not safe to repeat implicitly. */
export async function runUndoRestore(
  restoreFiles: RestoreStep,
  restorePosition: RestoreStep
): Promise<UndoRestoreResult> {
  let stage: 'files' | 'position' = 'files';
  try {
    const files = await restoreFiles();
    if (files.status === 'error') return { ...files, stage };
    stage = 'position';
    const position = await restorePosition();
    return position.status === 'error' ? { ...position, stage } : { status: 'ok' };
  } catch (error) {
    return { status: 'error', stage, error };
  }
}
