import { stopSharedTestWeb } from './rgsm-instance';

export default async function globalTeardown(): Promise<void> {
  await stopSharedTestWeb();
}
