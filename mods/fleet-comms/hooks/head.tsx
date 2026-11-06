import type { ClientModule } from "claude-code";

export type HeadProps = { label: string; isOpen: boolean };
export type HeadState = { hover: boolean; down: boolean };

export const HEAD_LOOK = {
  rest: { bg: "rgb(41,46,66)", fg: "rgb(187,154,247)" },
  open: { bg: "rgb(187,154,247)", fg: "rgb(26,27,38)" },
  hover: { bg: "rgb(122,162,247)", fg: "rgb(26,27,38)" },
  down: { bg: "rgb(192,202,245)", fg: "rgb(26,27,38)" },
} as const;

export function headText(label: string, isOpen: boolean): string {
  return ` ${label} ${isOpen ? "▾" : "▸"} `;
}

export function headWidth(label: string): number {
  return headText(label, false).length;
}

export function headLook(
  isOpen: boolean,
  state: HeadState,
): { bg: string; fg: string } {
  if (state.down) return HEAD_LOOK.down;
  if (state.hover) return HEAD_LOOK.hover;
  return isOpen ? HEAD_LOOK.open : HEAD_LOOK.rest;
}

const AT_REST: HeadState = { hover: false, down: false };

const Head: ClientModule<HeadProps, HeadState> = (props, s) => {
  const { Text } = s.elements;
  const label = typeof props?.label === "string" ? props.label : "mail";
  const isOpen = props?.isOpen === true;
  if (s.state === undefined) {
    s.onPointer((e) => {
      const was = s.state ?? AT_REST;
      const isOver = e.y === 0 && e.x >= 0 && e.x < headWidth(label);
      if (e.type === "leave") return s.setState(AT_REST);
      if (e.type === "down")
        return s.setState({ hover: isOver, down: isOver && e.button === "left" });
      if (e.type === "up") {
        if (was.down && isOver) s.post({ toggle: true });
        return s.setState({ hover: isOver, down: false });
      }
      if (was.hover !== isOver) s.setState({ hover: isOver, down: was.down });
    });
  }
  const look = headLook(isOpen, s.state ?? AT_REST);
  return (
    <Text bold backgroundColor={look.bg} color={look.fg}>
      {headText(label, isOpen)}
    </Text>
  );
};

export default Head;
