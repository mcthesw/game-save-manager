import { buildRgsmBinary } from './rgsm-instance';

export default async function globalSetup(): Promise<void> {
  await buildRgsmBinary();
}
