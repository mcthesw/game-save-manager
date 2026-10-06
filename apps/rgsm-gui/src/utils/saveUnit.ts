import type {
  ManifestPathConstraints,
  SaveUnit,
  SaveUnitDraft,
  SaveUnitType,
} from '../api/commands';

type Unit = SaveUnit | SaveUnitDraft;

export function concreteSaveUnit(
  unitType: SaveUnitType,
  paths: Record<string, string> = {},
  options: Omit<SaveUnitDraft, 'source'> = {}
): SaveUnitDraft {
  return {
    ...options,
    source: { type: 'devicePaths', unit_type: unitType, paths },
  };
}

export function manifestSaveUnit(
  pattern: string,
  options: Omit<SaveUnitDraft, 'source'> = {},
  constraints: ManifestPathConstraints = { alternatives: [] }
): SaveUnitDraft {
  return {
    ...options,
    source: { type: 'manifestPattern', pattern, constraints },
  };
}

export function saveUnitPaths(unit: Unit): Partial<Record<string, string>> | undefined {
  return unit.source.type === 'devicePaths' ? unit.source.paths : undefined;
}

export function saveUnitType(unit: Unit): SaveUnitType | undefined {
  return unit.source.type === 'devicePaths'
    ? unit.source.unit_type
    : (unit.source.expected_type ?? undefined);
}

export function saveUnitPattern(unit: Unit): string | undefined {
  return unit.source.type === 'manifestPattern' ? unit.source.pattern : undefined;
}

/** A picker returns a literal filesystem path, not a glob expression. */
export function escapePathLiteral(path: string): string {
  return path.replace(/\\/g, '/').replace(/[?*[\]{}]/g, (char) => `[${char}]`);
}
