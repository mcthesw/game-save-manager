import type { CloudNamespaceGeneration } from '../bindings';

export type CloudUiMode = 'loading' | 'legacy' | 'v2';

export function resolveCloudUiMode(generation: CloudNamespaceGeneration | null): CloudUiMode {
  if (generation === 'v2') return 'v2';
  if (generation === 'legacy_v1') return 'legacy';
  return 'loading';
}
