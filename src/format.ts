export function formatDuration(s: number) {
  return s < 60 ? `${s} seconds` : s === 60 ? "1 minute" : `${s / 60} minutes`;
}
