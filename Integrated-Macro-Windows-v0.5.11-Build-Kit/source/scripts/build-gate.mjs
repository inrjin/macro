// Shared by the verification runner and regression tests. No commands run here.
export function changedSources(before, after) {
  return [...new Set([...Object.keys(before), ...Object.keys(after)])]
    .filter((path) => before[path] !== after[path])
    .sort();
}

export function evaluateBuildGate(checks, initialFingerprints, currentFingerprints) {
  const blockedBy = checks
    .filter((check) => check.required && check.status !== 'passed')
    .map(({ name, status }) => ({ name, status }));
  const changed = changedSources(initialFingerprints, currentFingerprints);
  return {
    allowed: blockedBy.length === 0 && changed.length === 0,
    blockedBy,
    changedSources: changed,
  };
}
