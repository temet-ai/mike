export type Senders = Record<string, string>

declare module 'claude-code' {
  interface PluginState {
    'mike-mail': { senders: Senders }
  }
}
