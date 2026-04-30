import { useState, useEffect } from 'react'
import { Settings } from 'lucide-react'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogTrigger,
  DialogContent,
  DialogTitle,
  DialogDescription,
  DialogClose,
} from '@/components/ui/dialog'
import { useDeviceMcpConfig, useUpdateMcpConfig } from '@/api/hooks'
import type { McpConfig } from '@/api/types'
import { toast } from 'sonner'

interface McpConfigDialogProps {
  deviceId: string
  deviceName: string
}

export function McpConfigDialog({ deviceId, deviceName }: McpConfigDialogProps) {
  const [open, setOpen] = useState(false)
  const [configText, setConfigText] = useState('')
  const [error, setError] = useState<string | null>(null)

  const { data: configData, isLoading } = useDeviceMcpConfig(deviceId)
  const updateConfig = useUpdateMcpConfig()

  useEffect(() => {
    if (configData?.data) {
      setConfigText(JSON.stringify(configData.data, null, 2))
      setError(null)
    }
  }, [configData])

  const handleSave = () => {
    try {
      const config: McpConfig = JSON.parse(configText)
      setError(null)
      updateConfig.mutate(
        { deviceId, config },
        {
          onSuccess: () => {
            toast.success('MCP config updated')
            setOpen(false)
          },
          onError: () => {
            toast.error('Failed to update MCP config')
          },
        }
      )
    } catch {
      setError('Invalid JSON')
    }
  }

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger
        render={
          <Button variant="outline" size="sm">
            <Settings className="h-4 w-4 mr-2" />
            Configure
          </Button>
        }
      />
      <DialogContent className="sm:max-w-2xl">
        <DialogTitle>MCP Configuration - {deviceName}</DialogTitle>
        <DialogDescription>
          Configure MCP servers for this device. Changes will be applied on next restart.
        </DialogDescription>

        <div className="mt-4 space-y-4">
          {isLoading ? (
            <div className="h-64 flex items-center justify-center text-muted-foreground">
              Loading...
            </div>
          ) : (
            <>
              <textarea
                className="w-full h-64 font-mono text-sm p-3 rounded-md border bg-muted resize-none"
                value={configText}
                onChange={(e) => {
                  setConfigText(e.target.value)
                  setError(null)
                }}
                placeholder='{"servers": {}}'
              />
              {error && <p className="text-sm text-destructive">{error}</p>}
            </>
          )}

          <div className="flex justify-end gap-2">
            <DialogClose render={<Button variant="outline">Cancel</Button>} />
            <Button onClick={handleSave} disabled={updateConfig.isPending}>
              {updateConfig.isPending ? 'Saving...' : 'Save'}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  )
}
