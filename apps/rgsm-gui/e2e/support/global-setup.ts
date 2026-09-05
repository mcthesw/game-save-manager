import { prepareRgsmBuild } from './rgsm-instance';

export default async function globalSetup(): Promise<void> {
  prepareRgsmBuild();
}
