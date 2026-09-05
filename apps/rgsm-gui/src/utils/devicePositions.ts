type Positions = Record<string, string | null | undefined>;

/** Remote advertisements never recreate a cleared local current position. */
export function devicePositions(
  local: Positions | undefined,
  advertised: Positions | undefined,
  currentDeviceId: string | undefined
): Record<string, string> {
  const positions = Object.entries(advertised ?? local ?? {}).filter(
    (entry): entry is [string, string] =>
      entry[0] !== currentDeviceId && typeof entry[1] === 'string' && entry[1].length > 0
  );
  const own =
    currentDeviceId && local && Object.hasOwn(local, currentDeviceId)
      ? local[currentDeviceId]
      : undefined;
  if (currentDeviceId && own) positions.push([currentDeviceId, own]);
  return Object.fromEntries(positions);
}
