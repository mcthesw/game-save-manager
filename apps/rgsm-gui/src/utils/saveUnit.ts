import type { ManifestPathConstraints, SaveUnit, SaveUnitDraft, SaveUnitType } from '../bindings';

type Unit = SaveUnit | SaveUnitDraft;

export function concreteSaveUnit(
  unitType: SaveUnitType,
  paths: Record<string, string> = {},
  options: Omit<SaveUnitDraft, 'source'> = {}
): SaveUnitDraft {
  return {
    ...options,
    source: { type: 'concrete', unit_type: unitType, paths },
  };
}

export function manifestSaveUnit(
  pattern: string,
  options: Omit<SaveUnitDraft, 'source'> = {},
  constraints: ManifestPathConstraints = { os: [], stores: [] }
): SaveUnitDraft {
  return {
    ...options,
    source: { type: 'manifestPattern', pattern, constraints },
  };
}

export function saveUnitPaths(unit: Unit): Partial<Record<string, string>> | undefined {
  return unit.source.type === 'concrete' ? unit.source.paths : undefined;
}

export function saveUnitType(unit: Unit): SaveUnitType | undefined {
  return unit.source.type === 'concrete' ? unit.source.unit_type : undefined;
}

export function saveUnitPattern(unit: Unit): string | undefined {
  return unit.source.type === 'manifestPattern' ? unit.source.pattern : undefined;
}
