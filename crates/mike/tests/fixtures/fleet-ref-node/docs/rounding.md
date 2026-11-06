# Rounding

`roundTo(value, digits)` rounds to `digits` decimal places.

- Halves round away from zero: 2.5 becomes 3 and -2.5 becomes -3.
- `digits` is a whole number from 0 to 10; anything else throws a RangeError.
