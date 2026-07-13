export function visibleRange(
  scrollTop: number,
  viewportHeight: number,
  rowHeight: number,
  total: number,
  overscan = 10,
): { start: number; end: number } {
  const first = Math.min(Math.floor(scrollTop / rowHeight), total);
  const visible = Math.ceil(viewportHeight / rowHeight);
  const start = Math.max(0, first - overscan);
  const end = Math.min(total, first + visible + overscan);
  return { start, end };
}
