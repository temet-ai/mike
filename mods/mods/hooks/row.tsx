import type { ClientModule } from 'claude-code'

export type Chip = { label: string; command: string; dim?: boolean; alert?: boolean }
export type RowProps = { chips: Chip[] }
export type RowState = { hover: number | null; down: number | null }

export const LOOK = {
  rest: { bg: '#262A3D', fg: '#C9D0FA' },
  dim: { bg: '#1E2130', fg: '#8A93BD' },
  alert: { bg: '#5A2430', fg: '#FFD6DE' },
  hover: { bg: '#3B4C86', fg: '#FFFFFF' },
  down: { bg: '#C9D0FA', fg: '#1A1B26' },
} as const

export function chipWidth(chip: Chip): number {
  return chip.label.length + 2
}

export function rowWidth(chips: Chip[]): number {
  return chips.reduce((n, chip) => n + chipWidth(chip), 0) + Math.max(0, chips.length - 1)
}

export function chipAt(chips: Chip[], x: number, y: number): number | null {
  if (y !== 0 || x < 0) return null
  let left = 0
  for (let i = 0; i < chips.length; i++) {
    const chip = chips[i]
    if (chip === undefined) return null
    const right = left + chipWidth(chip)
    if (x >= left && x < right) return i
    left = right + 1
  }
  return null
}

export function lookOf(chip: Chip, i: number, state: RowState): { bg: string; fg: string } {
  if (state.down === i) return LOOK.down
  if (state.hover === i) return LOOK.hover
  if (chip.alert) return LOOK.alert
  if (chip.dim) return LOOK.dim
  return LOOK.rest
}

const AT_REST: RowState = { hover: null, down: null }

const Row: ClientModule<RowProps, RowState> = (props, s) => {
  const { Box, Text } = s.elements
  const chips = Array.isArray(props?.chips) ? props.chips : []
  if (s.state === undefined) {
    s.onPointer(e => {
      const was = s.state ?? AT_REST
      const at = chipAt(chips, e.x, e.y)
      if (e.type === 'leave') return s.setState({ hover: null, down: null })
      if (e.type === 'down') return s.setState({ hover: at, down: e.button === 'left' ? at : null })
      if (e.type === 'up') {
        const pressed = was.down !== null && was.down === at ? chips[was.down] : undefined
        if (pressed !== undefined) s.post({ press: pressed.command })
        return s.setState({ hover: at, down: null })
      }
      if (was.hover !== at) s.setState({ hover: at, down: was.down })
    })
  }
  const state = s.state ?? AT_REST
  return (
    <Box columnGap={1}>
      {chips.map((chip, i) => {
        const look = lookOf(chip, i, state)
        return (
          <Text backgroundColor={look.bg} color={look.fg} bold={!chip.dim}>
            {` ${chip.label} `}
          </Text>
        )
      })}
    </Box>
  )
}

export default Row
