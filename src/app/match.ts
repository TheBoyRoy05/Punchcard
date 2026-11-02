// Fuzzy match like choose: every query character appears in the name, in order, ignoring case.
export function matches(query: string, name: string): boolean {
  const target = name.toLowerCase();
  let from = 0;
  for (const char of query.toLowerCase()) {
    from = target.indexOf(char, from) + 1;
    if (from === 0) {
      return false;
    }
  }
  return true;
}
