import {
  MoreHorizontal,
  RefreshCw,
  Trash2,
  Terminal,
  CheckCircle2,
  AlertCircle,
  Circle,
  ExternalLink,
} from 'lucide-react'
import { Card, CardContent, CardHeader } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { CopyButton } from '@/components/copy-button'
import { McpConfigDialog } from './mcp-config-dialog'
import type { Device } from '@/api/types'

interface DeviceCardProps {
  device: Device
  onDelete: (id: string) => void
  onRegenerateKey: (id: string) => void
  onRestart: (key: string) => void
  onOpenSettings: (key: string, permission: string) => void
}

export function DeviceCard({
  device,
  onDelete,
  onRegenerateKey,
  onRestart,
  onOpenSettings,
}: DeviceCardProps) {
  return (
    <Card className={device.hasIssues ? 'border-yellow-500/50' : ''}>
      <CardHeader className="pb-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <div>
              <h3 className="font-semibold">{device.name}</h3>
              <code className="text-xs text-muted-foreground">{device.alias}</code>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <Badge variant={device.isOnline ? 'default' : 'secondary'}>
              <Circle
                className={`h-2 w-2 mr-1 ${device.isOnline ? 'fill-green-500 text-green-500' : 'fill-muted-foreground text-muted-foreground'}`}
              />
              {device.isOnline ? 'Online' : 'Offline'}
            </Badge>
            <DropdownMenu>
              <DropdownMenuTrigger
                render={<Button variant="ghost" size="icon" className="h-8 w-8" />}
              >
                <MoreHorizontal className="h-4 w-4" />
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuItem onClick={() => onRegenerateKey(device.id)}>
                  <RefreshCw className="h-4 w-4 mr-2" />
                  Regenerate Key
                </DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem
                  className="text-destructive"
                  onClick={() => onDelete(device.id)}
                >
                  <Trash2 className="h-4 w-4 mr-2" />
                  Delete
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        </div>
      </CardHeader>

      <CardContent className="space-y-4">
        {device.isOnline && device.hasIssues && (
          <PermissionsAlert
            permissions={device.permissions}
            deviceKey={device.key}
            onOpenSettings={onOpenSettings}
            onRestart={onRestart}
          />
        )}

        {device.isOnline && !device.hasIssues && device.permissions.length > 0 && (
          <div className="flex items-center gap-2 text-sm text-green-600">
            <CheckCircle2 className="h-4 w-4" />
            All permissions granted
          </div>
        )}

        {device.isOnline && device.mcpServers.length > 0 && (
          <McpServersStatus servers={device.mcpServers} />
        )}

        <div className="flex items-center justify-between">
          <span className="text-xs text-muted-foreground">
            {device.mcpConfig.length} MCP server{device.mcpConfig.length !== 1 ? 's' : ''} configured
          </span>
          <McpConfigDialog deviceId={device.id} deviceName={device.name} />
        </div>

        <div className="space-y-2">
          <label className="text-xs text-muted-foreground">Install command</label>
          <div className="flex items-center gap-2">
            <code className="flex-1 text-xs bg-muted px-3 py-2 rounded-md truncate">
              {device.installCommand}
            </code>
            <CopyButton value={device.installCommand} />
          </div>
        </div>
      </CardContent>
    </Card>
  )
}

function PermissionsAlert({
  permissions,
  deviceKey,
  onOpenSettings,
  onRestart,
}: {
  permissions: Device['permissions']
  deviceKey: string
  onOpenSettings: (key: string, permission: string) => void
  onRestart: (key: string) => void
}) {
  const missingPermissions = permissions.filter((p) => !p.granted)

  return (
    <div className="rounded-lg border border-yellow-500/50 bg-yellow-500/10 p-4 space-y-4">
      <div className="flex items-start gap-2">
        <AlertCircle className="h-5 w-5 text-yellow-600 mt-0.5" />
        <div>
          <h4 className="font-medium text-yellow-800 dark:text-yellow-200">
            Setup required
          </h4>
          <p className="text-sm text-yellow-700 dark:text-yellow-300">
            Portal needs permission to access data on your Mac.
          </p>
        </div>
      </div>

      <div className="space-y-2">
        {missingPermissions.map((perm) => (
          <div
            key={perm.name}
            className="flex items-center justify-between bg-background rounded-md p-3"
          >
            <div>
              <p className="text-sm font-medium">
                {perm.name === 'full_disk_access'
                  ? 'Full Disk Access'
                  : perm.name === 'accessibility'
                    ? 'Accessibility'
                    : perm.name}
              </p>
              <p className="text-xs text-muted-foreground">
                {perm.name === 'full_disk_access'
                  ? 'Read Messages, Mail, and other app data'
                  : perm.name === 'accessibility'
                    ? 'Control apps on your Mac'
                    : 'Required for Portal'}
              </p>
            </div>
            {perm.settingsUrl && (
              <Button
                size="sm"
                variant="outline"
                onClick={() => onOpenSettings(deviceKey, perm.name)}
              >
                <ExternalLink className="h-4 w-4 mr-1" />
                Open Settings
              </Button>
            )}
          </div>
        ))}
      </div>

      <Button size="sm" onClick={() => onRestart(deviceKey)}>
        <RefreshCw className="h-4 w-4 mr-2" />
        Restart & Verify
      </Button>
    </div>
  )
}

function McpServersStatus({ servers }: { servers: Device['mcpServers'] }) {
  return (
    <div className="space-y-2">
      <label className="text-xs text-muted-foreground">MCP Servers</label>
      <div className="flex flex-wrap gap-2">
        {servers.map((server) => (
          <Badge
            key={server.name}
            variant={server.running ? 'default' : 'destructive'}
            className="gap-1"
          >
            <Terminal className="h-3 w-3" />
            {server.name}
            {server.running && server.toolsCount > 0 && (
              <span className="text-xs opacity-70">({server.toolsCount})</span>
            )}
          </Badge>
        ))}
      </div>
    </div>
  )
}
