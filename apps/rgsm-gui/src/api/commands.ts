import './client';
import { updateAuthorizationToken } from './client';
import * as sdk from './generated/sdk.gen';
import type * as types from './generated/types.gen';
export { DEFAULT_CONFIG } from './defaultConfig.generated';
export { events } from './events';
export type {
  CloudSyncErrorEvent,
  CloudSyncStatusEvent,
  HostNotification,
  QuickActionCompleted,
} from './events';

type CommandResult<T> = { status: 'ok'; data: T } | { status: 'error'; error: string };

function unwrapDirect<T>(result: { data?: T; error?: unknown }): T {
  if (result.error !== undefined) {
    const error = result.error as Partial<types.ApiError>;
    throw new Error(error.message ?? String(result.error));
  }
  return result.data as T;
}

function unwrapRestore<T>(result: {
  data?: T;
  error?: unknown;
}): { status: 'ok'; data: T } | { status: 'error'; error: types.RestoreError } {
  if (result.error !== undefined) {
    const error = result.error as Partial<types.ApiError>;
    return {
      status: 'error',
      error: (error.details as types.RestoreError | undefined) ?? {
        type: 'Other',
        message: error.message ?? String(result.error),
      },
    };
  }
  return { status: 'ok', data: result.data as T };
}

function unwrap<T>(result: { data?: T; error?: unknown }): CommandResult<T> {
  if (result.error !== undefined) {
    const error = result.error as Partial<types.ApiError>;
    return { status: 'error', error: error.message ?? String(result.error) };
  }
  return { status: 'ok', data: result.data as T };
}

export const commands = {
  async getHttpHostInfo() {
    return unwrap<types.GetHttpHostInfoResponses[200]>(await sdk.getHttpHostInfo());
  },
  async regenerateHttpApiToken() {
    const result = unwrap<types.RegenerateHttpApiTokenResponses[200]>(
      await sdk.regenerateHttpApiToken()
    );
    if (result.status === 'ok') updateAuthorizationToken(result.data.token);
    return result;
  },
  async openUrl(url: types.OpenUrlRequest['url']) {
    return unwrap<types.OpenUrlResponses[200]>(await sdk.openUrl({ body: { url } }));
  },
  async getBuildInfo() {
    return unwrapDirect<types.GetBuildInfoResponses[200]>(await sdk.getBuildInfo());
  },
  async openFileOrFolder(path: types.OpenFileOrFolderRequest['path']) {
    return unwrap<types.OpenFileOrFolderResponses[200]>(
      await sdk.openFileOrFolder({ body: { path } })
    );
  },
  async getAppLogDir() {
    return unwrap<types.GetAppLogDirResponses[200]>(await sdk.getAppLogDir());
  },
  async chooseSaveFile() {
    return unwrap<types.ChooseSaveFileResponses[200]>(await sdk.chooseSaveFile());
  },
  async chooseSaveDir() {
    return unwrap<types.ChooseSaveDirResponses[200]>(await sdk.chooseSaveDir());
  },
  async getLocalConfig() {
    return unwrap<types.GetLocalConfigResponses[200]>(await sdk.getLocalConfig());
  },
  async addGame(game: types.AddGameRequest['game']) {
    return unwrap<types.AddGameResponses[200]>(await sdk.addGame({ body: { game } }));
  },
  async updateGame(
    storageKey: types.UpdateGameRequest['storageKey'],
    game: types.UpdateGameRequest['game']
  ) {
    return unwrap<types.UpdateGameResponses[200]>(
      await sdk.updateGame({ body: { storageKey, game } })
    );
  },
  async restoreSnapshot(
    game: types.RestoreSnapshotRequest['game'],
    date: types.RestoreSnapshotRequest['date']
  ) {
    return unwrapRestore<types.RestoreSnapshotResponses[200]>(
      await sdk.restoreSnapshot({ body: { game, date } })
    );
  },
  async deleteSnapshot(
    game: types.DeleteSnapshotRequest['game'],
    date: types.DeleteSnapshotRequest['date']
  ) {
    return unwrap<types.DeleteSnapshotResponses[200]>(
      await sdk.deleteSnapshot({ body: { game, date } })
    );
  },
  async batchDeleteSnapshots(
    game: types.BatchDeleteSnapshotsRequest['game'],
    dates: types.BatchDeleteSnapshotsRequest['dates']
  ) {
    return unwrap<types.BatchDeleteSnapshotsResponses[200]>(
      await sdk.batchDeleteSnapshots({ body: { game, dates } })
    );
  },
  async getCloudNamespaceGeneration() {
    return unwrap<types.GetCloudNamespaceGenerationResponses[200]>(
      await sdk.getCloudNamespaceGeneration()
    );
  },
  async deleteGame(game: types.DeleteGameRequest['game']) {
    return unwrap<types.DeleteGameResponses[200]>(await sdk.deleteGame({ body: { game } }));
  },
  async getGameSnapshotsInfo(game: types.GetGameSnapshotsInfoRequest['game']) {
    return unwrap<types.GetGameSnapshotsInfoResponses[200]>(
      await sdk.getGameSnapshotsInfo({ body: { game } })
    );
  },
  async verifyArchiveIntegrity(
    archivePath: types.VerifyArchiveIntegrityRequest['archivePath'],
    expectedHash: types.VerifyArchiveIntegrityRequest['expectedHash']
  ) {
    return unwrap<types.VerifyArchiveIntegrityResponses[200]>(
      await sdk.verifyArchiveIntegrity({ body: { archivePath, expectedHash } })
    );
  },
  async setConfig(config: types.SetConfigRequest['config']) {
    return unwrap<types.SetConfigResponses[200]>(await sdk.setConfig({ body: { config } }));
  },
  async resetSettings() {
    return unwrap<types.ResetSettingsResponses[200]>(await sdk.resetSettings());
  },
  async createSnapshot(
    game: types.CreateSnapshotRequest['game'],
    describe: types.CreateSnapshotRequest['describe']
  ) {
    return unwrap<types.CreateSnapshotResponses[200]>(
      await sdk.createSnapshot({ body: { game, describe } })
    );
  },
  async openBackupFolder(game: types.OpenBackupFolderRequest['game']) {
    return unwrap<types.OpenBackupFolderResponses[200]>(
      await sdk.openBackupFolder({ body: { game } })
    );
  },
  async getGameExtraBackups(game: types.GetGameExtraBackupsRequest['game']) {
    return unwrap<types.GetGameExtraBackupsResponses[200]>(
      await sdk.getGameExtraBackups({ body: { game } })
    );
  },
  async deleteExtraBackup(
    game: types.DeleteExtraBackupRequest['game'],
    date: types.DeleteExtraBackupRequest['date']
  ) {
    return unwrap<types.DeleteExtraBackupResponses[200]>(
      await sdk.deleteExtraBackup({ body: { game, date } })
    );
  },
  async restoreExtraBackup(
    game: types.RestoreExtraBackupRequest['game'],
    date: types.RestoreExtraBackupRequest['date']
  ) {
    return unwrap<types.RestoreExtraBackupResponses[200]>(
      await sdk.restoreExtraBackup({ body: { game, date } })
    );
  },
  async openExtraBackupFolder(game: types.OpenExtraBackupFolderRequest['game']) {
    return unwrap<types.OpenExtraBackupFolderResponses[200]>(
      await sdk.openExtraBackupFolder({ body: { game } })
    );
  },
  async checkCloudBackend(session: types.CheckCloudBackendRequest['session']) {
    return unwrap<types.CheckCloudBackendResponses[200]>(
      await sdk.checkCloudBackend({ body: { session } })
    );
  },
  async inspectCloudLibrary() {
    return unwrap<types.InspectCloudLibraryResponses[200]>(await sdk.inspectCloudLibrary());
  },
  async createCloudLibrary(confirmed: types.CreateCloudLibraryRequest['confirmed']) {
    return unwrap<types.CreateCloudLibraryResponses[200]>(
      await sdk.createCloudLibrary({ body: { confirmed } })
    );
  },
  async reviewCloudLibraryJoin() {
    return unwrap<types.ReviewCloudLibraryJoinResponses[200]>(await sdk.reviewCloudLibraryJoin());
  },
  async joinCloudLibrary(
    decisions: types.JoinCloudLibraryRequest['decisions'],
    confirmedReplacements: types.JoinCloudLibraryRequest['confirmedReplacements']
  ) {
    return unwrap<types.JoinCloudLibraryResponses[200]>(
      await sdk.joinCloudLibrary({ body: { decisions, confirmedReplacements } })
    );
  },
  async reviewCloudLibraryCutover() {
    return unwrap<types.ReviewCloudLibraryCutoverResponses[200]>(
      await sdk.reviewCloudLibraryCutover()
    );
  },
  async cutoverCloudLibrary(confirmed: types.CutoverCloudLibraryRequest['confirmed']) {
    return unwrap<types.CutoverCloudLibraryResponses[200]>(
      await sdk.cutoverCloudLibrary({ body: { confirmed } })
    );
  },
  async getCloudArchiveLibrary() {
    return unwrap<types.GetCloudArchiveLibraryResponses[200]>(await sdk.getCloudArchiveLibrary());
  },
  async reviewV2GameProgress(gameId: types.ReviewV2GameProgressRequest['gameId']) {
    return unwrap<types.ReviewV2GameProgressResponses[200]>(
      await sdk.reviewV2GameProgress({ body: { gameId } })
    );
  },
  async keepV2LocalProgress(
    gameId: types.KeepV2LocalProgressRequest['gameId'],
    manifestRevision: types.KeepV2LocalProgressRequest['manifestRevision'],
    localSnapshotId: types.KeepV2LocalProgressRequest['localSnapshotId']
  ) {
    return unwrap<types.KeepV2LocalProgressResponses[200]>(
      await sdk.keepV2LocalProgress({ body: { gameId, manifestRevision, localSnapshotId } })
    );
  },
  async acceptV2RemoteProgress(
    gameId: types.AcceptV2RemoteProgressRequest['gameId'],
    manifestRevision: types.AcceptV2RemoteProgressRequest['manifestRevision'],
    expectedLocalSnapshotId: types.AcceptV2RemoteProgressRequest['expectedLocalSnapshotId'],
    selectedSnapshotId: types.AcceptV2RemoteProgressRequest['selectedSnapshotId']
  ) {
    return unwrap<types.AcceptV2RemoteProgressResponses[200]>(
      await sdk.acceptV2RemoteProgress({
        body: { gameId, manifestRevision, expectedLocalSnapshotId, selectedSnapshotId },
      })
    );
  },
  async previewMaterializeAll() {
    return unwrap<types.PreviewMaterializeAllResponses[200]>(await sdk.previewMaterializeAll());
  },
  async uploadCloudArchive(
    gameId: types.UploadCloudArchiveRequest['gameId'],
    snapshotId: types.UploadCloudArchiveRequest['snapshotId']
  ) {
    return unwrap<types.UploadCloudArchiveResponses[200]>(
      await sdk.uploadCloudArchive({ body: { gameId, snapshotId } })
    );
  },
  async downloadCloudArchive(
    gameId: types.DownloadCloudArchiveRequest['gameId'],
    snapshotId: types.DownloadCloudArchiveRequest['snapshotId']
  ) {
    return unwrap<types.DownloadCloudArchiveResponses[200]>(
      await sdk.downloadCloudArchive({ body: { gameId, snapshotId } })
    );
  },
  async deleteV2Snapshot(
    gameId: types.DeleteV2SnapshotRequest['gameId'],
    snapshotId: types.DeleteV2SnapshotRequest['snapshotId'],
    confirmed: types.DeleteV2SnapshotRequest['confirmed']
  ) {
    return unwrap<types.DeleteV2SnapshotResponses[200]>(
      await sdk.deleteV2Snapshot({ body: { gameId, snapshotId, confirmed } })
    );
  },
  async setSharedSnapshotRetention(
    gameId: types.SetSharedSnapshotRetentionRequest['gameId'],
    limit: types.SetSharedSnapshotRetentionRequest['limit'],
    confirmed: types.SetSharedSnapshotRetentionRequest['confirmed']
  ) {
    return unwrap<types.SetSharedSnapshotRetentionResponses[200]>(
      await sdk.setSharedSnapshotRetention({ body: { gameId, limit, confirmed } })
    );
  },
  async setSnapshotRetentionProtected(
    gameId: types.SetSnapshotRetentionProtectedRequest['gameId'],
    snapshotId: types.SetSnapshotRetentionProtectedRequest['snapshotId'],
    retentionProtected: types.SetSnapshotRetentionProtectedRequest['retentionProtected'],
    confirmed: types.SetSnapshotRetentionProtectedRequest['confirmed']
  ) {
    return unwrap<types.SetSnapshotRetentionProtectedResponses[200]>(
      await sdk.setSnapshotRetentionProtected({
        body: { gameId, snapshotId, retentionProtected, confirmed },
      })
    );
  },
  async getCurrentDeviceGameStatuses() {
    return unwrap<types.GetCurrentDeviceGameStatusesResponses[200]>(
      await sdk.getCurrentDeviceGameStatuses()
    );
  },
  async setDeviceGameVisibility(
    gameId: types.SetDeviceGameVisibilityRequest['gameId'],
    visible: types.SetDeviceGameVisibilityRequest['visible']
  ) {
    return unwrap<types.SetDeviceGameVisibilityResponses[200]>(
      await sdk.setDeviceGameVisibility({ body: { gameId, visible } })
    );
  },
  async setDeviceGameManaged(
    gameId: types.SetDeviceGameManagedRequest['gameId'],
    managed: types.SetDeviceGameManagedRequest['managed'],
    confirmed: types.SetDeviceGameManagedRequest['confirmed']
  ) {
    return unwrap<types.SetDeviceGameManagedResponses[200]>(
      await sdk.setDeviceGameManaged({ body: { gameId, managed, confirmed } })
    );
  },
  async evictLocalArchive(
    gameId: types.EvictLocalArchiveRequest['gameId'],
    snapshotId: types.EvictLocalArchiveRequest['snapshotId'],
    confirmed: types.EvictLocalArchiveRequest['confirmed']
  ) {
    return unwrap<types.EvictLocalArchiveResponses[200]>(
      await sdk.evictLocalArchive({ body: { gameId, snapshotId, confirmed } })
    );
  },
  async getCloudDeviceProfiles() {
    return unwrap<types.GetCloudDeviceProfilesResponses[200]>(await sdk.getCloudDeviceProfiles());
  },
  async removeCloudDeviceProfile(
    deviceId: types.RemoveCloudDeviceProfileRequest['deviceId'],
    confirmed: types.RemoveCloudDeviceProfileRequest['confirmed']
  ) {
    return unwrap<types.RemoveCloudDeviceProfileResponses[200]>(
      await sdk.removeCloudDeviceProfile({ body: { deviceId, confirmed } })
    );
  },
  async getDeletedCloudGames() {
    return unwrap<types.GetDeletedCloudGamesResponses[200]>(await sdk.getDeletedCloudGames());
  },
  async permanentlyDeleteCloudGame(
    gameId: types.PermanentlyDeleteCloudGameRequest['gameId'],
    confirmed: types.PermanentlyDeleteCloudGameRequest['confirmed']
  ) {
    return unwrap<types.PermanentlyDeleteCloudGameResponses[200]>(
      await sdk.permanentlyDeleteCloudGame({ body: { gameId, confirmed } })
    );
  },
  async materializeAllCloudArchives() {
    return unwrap<types.MaterializeAllCloudArchivesResponses[200]>(
      await sdk.materializeAllCloudArchives()
    );
  },
  async setGameSyncMode(
    gameId: types.SetGameSyncModeRequest['gameId'],
    mode: types.SetGameSyncModeRequest['mode'],
    initialCatchUp: types.SetGameSyncModeRequest['initialCatchUp'],
    liveSave: types.SetGameSyncModeRequest['liveSave']
  ) {
    return unwrap<types.SetGameSyncModeResponses[200]>(
      await sdk.setGameSyncMode({ body: { gameId, mode, initialCatchUp, liveSave } })
    );
  },
  async cloudUploadAll(session: types.CloudUploadAllRequest['session']) {
    return unwrap<types.CloudUploadAllResponses[200]>(
      await sdk.cloudUploadAll({ body: { session } })
    );
  },
  async cloudDownloadAll(session: types.CloudDownloadAllRequest['session']) {
    return unwrap<types.CloudDownloadAllResponses[200]>(
      await sdk.cloudDownloadAll({ body: { session } })
    );
  },
  async cancelCloudSync() {
    return unwrap<types.CancelCloudSyncResponses[200]>(await sdk.cancelCloudSync());
  },
  async setSnapshotDescription(
    game: types.SetSnapshotDescriptionRequest['game'],
    date: types.SetSnapshotDescriptionRequest['date'],
    describe: types.SetSnapshotDescriptionRequest['describe']
  ) {
    return unwrap<types.SetSnapshotDescriptionResponses[200]>(
      await sdk.setSnapshotDescription({ body: { game, date, describe } })
    );
  },
  async backupAll() {
    return unwrap<types.BackupAllResponses[200]>(await sdk.backupAll());
  },
  async applyAll() {
    return unwrap<types.ApplyAllResponses[200]>(await sdk.applyAll());
  },
  async setQuickBackupGame(game: types.SetQuickBackupGameRequest['game']) {
    return unwrap<types.SetQuickBackupGameResponses[200]>(
      await sdk.setQuickBackupGame({ body: { game } })
    );
  },
  async setGameAutoBackup(
    gameName: types.SetGameAutoBackupRequest['gameName'],
    autoBackup: types.SetGameAutoBackupRequest['autoBackup']
  ) {
    return unwrap<types.SetGameAutoBackupResponses[200]>(
      await sdk.setGameAutoBackup({ body: { gameName, autoBackup } })
    );
  },
  async setGameAutomation(
    storageKey: types.SetGameAutomationRequest['storageKey'],
    automation: types.SetGameAutomationRequest['automation']
  ) {
    return unwrap<types.SetGameAutomationResponses[200]>(
      await sdk.setGameAutomation({ body: { storageKey, automation } })
    );
  },
  async setGameAutoSaveSettings(
    storageKey: types.SetGameAutoSaveSettingsRequest['storageKey'],
    autoBackup: types.SetGameAutoSaveSettingsRequest['autoBackup'],
    automation: types.SetGameAutoSaveSettingsRequest['automation']
  ) {
    return unwrap<types.SetGameAutoSaveSettingsResponses[200]>(
      await sdk.setGameAutoSaveSettings({ body: { storageKey, autoBackup, automation } })
    );
  },
  async setSnapshotCreatedBy(
    gameName: types.SetSnapshotCreatedByRequest['gameName'],
    snapshotDate: types.SetSnapshotCreatedByRequest['snapshotDate'],
    createdBy: types.SetSnapshotCreatedByRequest['createdBy']
  ) {
    return unwrap<types.SetSnapshotCreatedByResponses[200]>(
      await sdk.setSnapshotCreatedBy({ body: { gameName, snapshotDate, createdBy } })
    );
  },
  async getAutoBackupStatus() {
    return unwrap<types.GetAutoBackupStatusResponses[200]>(await sdk.getAutoBackupStatus());
  },
  async listRunningProcesses() {
    return unwrap<types.ListRunningProcessesResponses[200]>(await sdk.listRunningProcesses());
  },
  async resolvePath(path: types.ResolvePathRequest['path']) {
    return unwrap<types.ResolvePathResponses[200]>(await sdk.resolvePath({ body: { path } }));
  },
  async getCurrentDeviceInfo() {
    return unwrap<types.GetCurrentDeviceInfoResponses[200]>(await sdk.getCurrentDeviceInfo());
  },
  async toggleQuickActionSoundPreview(
    preferences: types.ToggleQuickActionSoundPreviewRequest['preferences'],
    effect: types.ToggleQuickActionSoundPreviewRequest['effect']
  ) {
    return unwrap<types.ToggleQuickActionSoundPreviewResponses[200]>(
      await sdk.toggleQuickActionSoundPreview({ body: { preferences, effect } })
    );
  },
  async stopSoundPlayback() {
    return unwrap<types.StopSoundPlaybackResponses[200]>(await sdk.stopSoundPlayback());
  },
  async chooseQuickActionSoundFile() {
    return unwrap<types.ChooseQuickActionSoundFileResponses[200]>(
      await sdk.chooseQuickActionSoundFile()
    );
  },
  async setSnapshotHead(
    game: types.SetSnapshotHeadRequest['game'],
    date: types.SetSnapshotHeadRequest['date']
  ) {
    return unwrap<types.SetSnapshotHeadResponses[200]>(
      await sdk.setSnapshotHead({ body: { game, date } })
    );
  },
  async detachSnapshot(
    game: types.DetachSnapshotRequest['game'],
    date: types.DetachSnapshotRequest['date']
  ) {
    return unwrap<types.DetachSnapshotResponses[200]>(
      await sdk.detachSnapshot({ body: { game, date } })
    );
  },
  async createSnapshotAt(
    game: types.CreateSnapshotAtRequest['game'],
    describe: types.CreateSnapshotAtRequest['describe'],
    parentDate: types.CreateSnapshotAtRequest['parentDate']
  ) {
    return unwrap<types.CreateSnapshotAtResponses[200]>(
      await sdk.createSnapshotAt({ body: { game, describe, parentDate } })
    );
  },
  async fetchLudusaviGames(filterLocalOnly: types.FetchLudusaviGamesRequest['filterLocalOnly']) {
    return unwrap<types.FetchLudusaviGamesResponses[200]>(
      await sdk.fetchLudusaviGames({ body: { filterLocalOnly } })
    );
  },
  async getGameSavePaths(gameName: types.GetGameSavePathsRequest['gameName']) {
    return unwrap<types.GetGameSavePathsResponses[200]>(
      await sdk.getGameSavePaths({ body: { gameName } })
    );
  },
  async getPathPlaceholderCatalog() {
    return unwrapDirect<types.GetPathPlaceholderCatalogResponses[200]>(
      await sdk.getPathPlaceholderCatalog()
    );
  },
  async previewSaveUnitResolution(
    game: types.PreviewSaveUnitResolutionRequest['game'],
    saveUnit: types.PreviewSaveUnitResolutionRequest['saveUnit']
  ) {
    return unwrap<types.PreviewSaveUnitResolutionResponses[200]>(
      await sdk.previewSaveUnitResolution({ body: { game, saveUnit } })
    );
  },
  async setGameDeviceBinding(
    identity: types.SetGameDeviceBindingRequest['identity'],
    binding: types.SetGameDeviceBindingRequest['binding']
  ) {
    return unwrap<types.SetGameDeviceBindingResponses[200]>(
      await sdk.setGameDeviceBinding({ body: { identity, binding } })
    );
  },
  async saveRestoreMapping(
    identity: types.SaveRestoreMappingRequest['identity'],
    saveUnitId: types.SaveRestoreMappingRequest['saveUnitId'],
    sourceDimensions: types.SaveRestoreMappingRequest['sourceDimensions'],
    targetCandidateIds: types.SaveRestoreMappingRequest['targetCandidateIds']
  ) {
    return unwrap<types.SaveRestoreMappingResponses[200]>(
      await sdk.saveRestoreMapping({
        body: { identity, saveUnitId, sourceDimensions, targetCandidateIds },
      })
    );
  },
  async getLudusaviManifestStatus() {
    return unwrap<types.GetLudusaviManifestStatusResponses[200]>(
      await sdk.getLudusaviManifestStatus()
    );
  },
  async updateLudusaviManifest() {
    return unwrap<types.UpdateLudusaviManifestResponses[200]>(await sdk.updateLudusaviManifest());
  },
  async resetLudusaviManifestToBundled() {
    return unwrap<types.ResetLudusaviManifestToBundledResponses[200]>(
      await sdk.resetLudusaviManifestToBundled()
    );
  },
  async checkPaths(
    paths: types.CheckPathsRequest['paths'],
    storeUserId: types.CheckPathsRequest['storeUserId'],
    installDirs: types.CheckPathsRequest['installDirs'],
    steamId: types.CheckPathsRequest['steamId']
  ) {
    return unwrap<types.CheckPathsResponses[200]>(
      await sdk.checkPaths({ body: { paths, storeUserId, installDirs, steamId } })
    );
  },
  async detectGameRoots() {
    return unwrap<types.DetectGameRootsResponses[200]>(await sdk.detectGameRoots());
  },
  async detectStoreUserIds() {
    return unwrap<types.DetectStoreUserIdsResponses[200]>(await sdk.detectStoreUserIds());
  },
  async getSystemFonts() {
    return unwrapDirect<types.GetSystemFontsResponses[200]>(await sdk.getSystemFonts());
  },
  async getSyncState() {
    return unwrap<types.GetSyncStateResponses[200]>(await sdk.getSyncState());
  },
  async scanVns(dirs: types.ScanVnsRequest['dirs']) {
    return unwrap<types.ScanVnsResponses[200]>(await sdk.scanVns({ body: { dirs } }));
  },
  async listConfigBackups() {
    return unwrapDirect<types.ListConfigBackupsResponses[200]>(await sdk.listConfigBackups());
  },
  async restoreConfigBackup(index: types.RestoreConfigBackupRequest['index']) {
    return unwrap<types.RestoreConfigBackupResponses[200]>(
      await sdk.restoreConfigBackup({ body: { index } })
    );
  },
  async syncGame(gameName: types.SyncGameRequest['gameName']) {
    return unwrap<types.SyncGameResponses[200]>(await sdk.syncGame({ body: { gameName } }));
  },
  async resolveGameSyncConflict(
    gameName: types.ResolveGameSyncConflictRequest['gameName'],
    resolution: types.ResolveGameSyncConflictRequest['resolution']
  ) {
    return unwrap<types.ResolveGameSyncConflictResponses[200]>(
      await sdk.resolveGameSyncConflict({ body: { gameName, resolution } })
    );
  },
  async syncConfig() {
    return unwrap<types.SyncConfigResponses[200]>(await sdk.syncConfig());
  },
};

export * from './generated/types.gen';
