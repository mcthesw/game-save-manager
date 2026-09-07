import { isEqual } from 'lodash-unified';

/** Keep unchanged records stable so unrelated metadata cannot reload their consumers. */
export function shareUnchangedItems<T>(current: T[], incoming: T[], key: (item: T) => string): T[] {
  if (isEqual(current, incoming)) return current;
  const previousByKey = new Map(current.map((item) => [key(item), item]));
  return incoming.map((item) => {
    const previous = previousByKey.get(key(item));
    return previous !== undefined && isEqual(previous, item) ? previous : item;
  });
}
