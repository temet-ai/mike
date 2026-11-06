export function clamp(value: number, min: number, max: number): number {
  if (min > max) throw new RangeError("min is above max");
  return Math.min(Math.max(value, min), max);
}
