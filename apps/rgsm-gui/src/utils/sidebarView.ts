export function initialGameListView(
  preference: 'favorites' | 'all' | null | undefined,
  hasFavorites: boolean
): 'favorites' | 'all' {
  return preference ?? (hasFavorites ? 'favorites' : 'all');
}
