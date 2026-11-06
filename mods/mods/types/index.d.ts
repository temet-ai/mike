export type FilterText = string

declare module 'claude-code' {
  interface PluginState {
    mods: { filter: FilterText; buttonsShown: boolean; statuses: Record<string, string>; hiddenCommands: string[] }
  }
}
