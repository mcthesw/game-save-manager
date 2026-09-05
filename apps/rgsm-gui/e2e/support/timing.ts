export function reportTiming(label: string, startedAt: number): void {
  if (process.env.CI || process.env.RGSM_E2E_TIMINGS) {
    console.log(`[e2e setup] ${label}: ${((performance.now() - startedAt) / 1000).toFixed(2)}s`);
  }
}
