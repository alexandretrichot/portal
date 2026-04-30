import { useState, useEffect, useRef } from 'react'
import { Settings } from 'lucide-react'
import Editor, { type OnMount } from '@monaco-editor/react'
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
import { api } from '@/api/client'
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
  const editorRef = useRef<Parameters<OnMount>[0] | null>(null)

  const { data: configData, isLoading } = useDeviceMcpConfig(deviceId)
  const updateConfig = useUpdateMcpConfig()

  useEffect(() => {
    if (configData?.data) {
      setConfigText(JSON.stringify(configData.data, null, 2))
      setError(null)
    }
  }, [configData])

  const handleEditorMount: OnMount = async (editor, monaco) => {
    editorRef.current = editor

    try {
      const schema = await api.get<object>('/api/schema/mcp-config')
      monaco.languages.json.jsonDefaults.setDiagnosticsOptions({
        validate: true,
        schemas: [
          {
            uri: 'https://portal.local/mcp-config.schema.json',
            fileMatch: ['*'],
            schema,
          },
        ],
      })
    } catch (e) {
      console.warn('Failed to load MCP config schema', e)
    }
  }

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
            <div className="h-80 flex items-center justify-center text-muted-foreground">
              Loading...
            </div>
          ) : (
            <div className="h-80 border rounded-md overflow-hidden">
              <Editor
                defaultLanguage="json"
                value={configText}
                onChange={(value) => {
                  setConfigText(value || '')
                  setError(null)
                }}
                onMount={handleEditorMount}
                options={{
                  minimap: { enabled: false },
                  fontSize: 13,
                  lineNumbers: 'off',
                  scrollBeyondLastLine: false,
                  automaticLayout: true,
                  tabSize: 2,
                }}
                theme="vs-dark"
              />
            </div>
          )}
          {error && <p className="text-sm text-destructive">{error}</p>}

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
