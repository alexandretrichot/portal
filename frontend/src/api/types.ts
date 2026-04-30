export interface Gateway {
  id: string
  key: string
  mcpUrl: string
}

export interface Device {
  id: string
  name: string
  alias: string
  key: string
  isOnline: boolean
  hasIssues: boolean
  permissions: Permission[]
  mcpServers: McpServer[]
  mcpConfig: McpConfigEntry[]
  installCommand: string
}

export interface Permission {
  name: string
  granted: boolean
  settingsUrl?: string
}

export interface McpServer {
  name: string
  running: boolean
  toolsCount: number
  error?: string
}

export interface McpConfigEntry {
  name: string
}

export interface McpConfig {
  servers: Record<string, McpServerConfig>
}

export interface McpServerConfig {
  type: 'stdio' | 'http'
  command?: string
  args?: string[]
  env?: Record<string, string>
  url?: string
  headers?: Record<string, string>
}

export interface DashboardData {
  baseUrl: string
  gateway: Gateway
  devices: Device[]
}

export interface ApiResponse<T> {
  success: boolean
  data?: T
  error?: string
}
