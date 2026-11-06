import type { Register } from 'claude-code'

import { statusColor } from './board-data'
import { CARD_TOOLS, cardAge, cardOf } from './card-data'

const LABEL_WIDTH = 9

export const register: Register = on => {
  on('ui.render', { component: 'ToolResult' }, async ($, e, next) => {
    const props = e.props as { tool?: string; output?: unknown; isErrored?: boolean; isExpanded?: boolean }
    if (typeof props.tool !== 'string' || !CARD_TOOLS.test(props.tool)) return next(e)
    if (props.isErrored === true) return next(e)
    const card = cardOf(props.tool, props.output)
    if (card === null) return next(e)

    const { Box, Text } = $.ui.resolve(e)
    const now = await $.clock.now()
    const line = (label: string, value: string, color?: string) => (
      <Box>
        <Box width={LABEL_WIDTH} flexShrink={0}>
          <Text dimColor>{label}</Text>
        </Box>
        <Text wrap="wrap" color={color}>
          {value}
        </Text>
      </Box>
    )
    const when = cardAge(card, now)
    const requestId = (e as { requestId?: string }).requestId ?? card.slug
    // Claude Code draws a ToolResult only in the ctrl+o view (isExpanded is always true there), so the card is drawn expanded too.

    return (
      <Box key={`card-${requestId}`} flexDirection="column" borderStyle="round" borderColor={card.parseError !== '' ? 'error' : 'subtle'} paddingX={1}>
        <Box justifyContent="space-between">
          <Box columnGap={1}>
            <Text bold>{card.title}</Text>
            {card.title !== card.slug && <Text dimColor>{card.slug}</Text>}
          </Box>
          <Box flexShrink={0}>
            <Text bold color={statusColor(card.status)}>
              {card.status}
            </Text>
            {when !== '' && <Text dimColor>{` · ${when}`}</Text>}
          </Box>
        </Box>
        {card.saved !== null && line('saved', card.saved)}
        {line('for', card.tldr)}
        {card.next === '' ? (
          <Box>
            <Box width={LABEL_WIDTH} flexShrink={0}>
              <Text dimColor>next</Text>
            </Box>
            <Text color="inactive">not written yet</Text>
          </Box>
        ) : (
          line('next', card.next)
        )}
        {card.blocked !== '' && line('blocked', card.blocked)}
        {card.questions !== '' && line('questions', card.questions)}
        {card.parseError !== '' && line('import', card.parseError, 'error')}
        {card.history !== null && line('history', card.history)}
      </Box>
    )
  })
}
