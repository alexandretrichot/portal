import { RefreshCw } from 'lucide-react'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { CopyButton } from '@/components/copy-button'

interface GatewayCardProps {
  mcpUrl: string
  onRegenerate: () => void
  isRegenerating?: boolean
}

export function GatewayCard({
  mcpUrl,
  onRegenerate,
  isRegenerating,
}: GatewayCardProps) {
  return (
    <Card>
      <CardHeader className="pb-3">
        <CardTitle className="text-base">MCP Gateway</CardTitle>
        <CardDescription>
          Configure your AI agent to use this endpoint
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="flex items-center gap-2">
          <code className="flex-1 text-sm bg-muted px-3 py-2 rounded-md truncate font-mono">
            {mcpUrl}
          </code>
          <CopyButton value={mcpUrl} />
        </div>
        <Button
          variant="outline"
          size="sm"
          onClick={onRegenerate}
          disabled={isRegenerating}
        >
          <RefreshCw className={`h-4 w-4 mr-2 ${isRegenerating ? 'animate-spin' : ''}`} />
          Regenerate Key
        </Button>
      </CardContent>
    </Card>
  )
}
