import { accessSync } from 'node:fs';

type BuildPaths = { binary: string; helper: string };

// Playwright passes global setup's environment to every worker, unlike module
// variables. This marker only lives for one run; it is not a disk build cache.
const preparedKey = 'RGSM_E2E_PREPARED_ARTIFACTS';

function checkArtifacts(paths: BuildPaths): void {
  accessSync(paths.binary);
  accessSync(paths.helper);
}

export function prepareBuild(env: NodeJS.ProcessEnv, paths: BuildPaths, build: () => void): void {
  delete env[preparedKey];
  build();
  checkArtifacts(paths);
  env[preparedKey] = JSON.stringify(paths);
}

export function preparedBinary(env: NodeJS.ProcessEnv, paths: BuildPaths): string {
  if (env[preparedKey] !== JSON.stringify(paths)) {
    throw new Error('E2E artifacts were not prepared by global setup for this checkout');
  }
  checkArtifacts(paths);
  return paths.binary;
}

export function buildEnvironment(env: NodeJS.ProcessEnv): NodeJS.ProcessEnv {
  return {
    ...env,
    ...(!env.CI && !env.CARGO_BUILD_JOBS ? { CARGO_BUILD_JOBS: '2' } : {}),
  };
}
