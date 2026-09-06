import { prepareRgsmBuild, workspacePath } from './rgsm-instance';
import { startTestWebServer } from './test-web';

export default async function globalSetup(): Promise<() => Promise<void>> {
  prepareRgsmBuild();
  const web = await startTestWebServer(workspacePath('apps', 'rgsm-gui'));
  return () => web.stop();
}
