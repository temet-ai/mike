export function roundTo(value: number, digits: number): number {
  if (!Number.isInteger(digits) || digits < 0 || digits > 10) {
    throw new RangeError("digits must be a whole number from 0 to 10");
  }
  const factor = 10 ** digits;
  const scaled = Math.abs(value) * factor;
  return (Math.sign(value) * Math.round(scaled + Number.EPSILON)) / factor;
}
