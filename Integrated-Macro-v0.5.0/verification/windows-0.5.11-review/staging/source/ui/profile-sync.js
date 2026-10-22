const clone = (value) => value === undefined ? undefined : structuredClone(value);
const object = (value) => value !== null && typeof value === 'object' && !Array.isArray(value);

// Components of a sampled color or calibrated position are one value. Merging
// their channels/axes separately can invent a color or point nobody selected.
const atomicCalibration = (value, path) => object(value) && (
  path.at(-1) === 'capture_reference'
  || ['r', 'g', 'b'].every(key => Object.hasOwn(value, key))
  || ['x', 'y'].every(key => Object.hasOwn(value, key))
);

export function sameProfile(left, right) {
  if (left === right) return true;
  if (Array.isArray(left) && Array.isArray(right)) {
    return left.length === right.length && left.every((value, index) => sameProfile(value, right[index]));
  }
  if (!object(left) || !object(right)) return false;
  const keys = Object.keys(left);
  return keys.length === Object.keys(right).length && keys.every((key) => Object.hasOwn(right, key) && sameProfile(left[key], right[key]));
}

// Keep local edits to fields that the worker has not changed. A newer native
// capture wins a simultaneous edit to the same field; stale coordinates must
// never be silently reintroduced while adopting its revision.
export function reconcileProfile(base, draft, current) {
  const conflicts = [];
  function merge(before, local, remote, path) {
    if (sameProfile(local, before)) return clone(remote);
    if (sameProfile(remote, before) || sameProfile(local, remote)) return clone(local);
    if ([before, local, remote].some(value => atomicCalibration(value, path))) {
      conflicts.push(path.join('.'));
      return clone(remote);
    }
    if (object(before) && object(local) && object(remote)) {
      const result = {};
      for (const key of new Set([...Object.keys(before), ...Object.keys(local), ...Object.keys(remote)])) {
        const value = merge(before[key], local[key], remote[key], [...path, key]);
        if (value !== undefined) Object.defineProperty(result, key, { value, enumerable: true, writable: true, configurable: true });
      }
      return result;
    }
    if ([before, local, remote].every(Array.isArray)) {
      // These two slots are fixed region identities, unlike arbitrary arrays.
      // F8 can create slot 2 while slot 1 has unsaved independent overrides.
      if (path.join('.') === 'legacy.regions') {
        const emptyRegion = () => ({ color: null, vision: null, complete: null });
        const result = Array.from({ length: Math.max(before.length, local.length, remote.length) }, (_, index) => {
          const base = before[index] === undefined && object(local[index]) && object(remote[index])
            ? emptyRegion() : before[index];
          return merge(base, local[index], remote[index], [...path, index]);
        });
        while (result.at(-1) === undefined && result.length) result.pop();
        return result.map(value => value === undefined ? emptyRegion() : value);
      }
      if (before.length === local.length && before.length === remote.length) {
        return local.map((value, index) => merge(before[index], value, remote[index], [...path, index]));
      }
    }
    conflicts.push(path.join('.'));
    return clone(remote);
  }
  return { profile: merge(base, draft, current, []), conflicts };
}
