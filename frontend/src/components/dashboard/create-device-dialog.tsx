import { useState } from 'react'
import { Plus } from 'lucide-react'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'

interface CreateDeviceDialogProps {
  onSubmit: (data: { name: string; alias: string }) => void
  isLoading?: boolean
}

export function CreateDeviceDialog({ onSubmit, isLoading }: CreateDeviceDialogProps) {
  const [open, setOpen] = useState(false)
  const [name, setName] = useState('')
  const [alias, setAlias] = useState('')

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    onSubmit({ name, alias: alias.toLowerCase() })
    setName('')
    setAlias('')
    setOpen(false)
  }

  const isValidAlias = /^[a-z0-9]{2,8}$/.test(alias.toLowerCase())

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger render={<Button />}>
        <Plus className="h-4 w-4 mr-2" />
        New Device
      </DialogTrigger>
      <DialogContent>
        <form onSubmit={handleSubmit}>
          <DialogHeader>
            <DialogTitle>Create Device</DialogTitle>
            <DialogDescription>
              Add a new device to connect your machine.
            </DialogDescription>
          </DialogHeader>

          <div className="grid gap-4 py-4">
            <div className="space-y-2">
              <Label htmlFor="name">Name</Label>
              <Input
                id="name"
                placeholder="My MacBook"
                value={name}
                onChange={(e) => setName(e.target.value)}
                required
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="alias">Alias</Label>
              <Input
                id="alias"
                placeholder="mac"
                value={alias}
                onChange={(e) => setAlias(e.target.value.toLowerCase())}
                pattern="[a-z0-9]{2,8}"
                required
              />
              <p className="text-xs text-muted-foreground">
                2-8 lowercase letters/numbers. Used in tool names (e.g., mac:read_file)
              </p>
            </div>
          </div>

          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => setOpen(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={!name || !isValidAlias || isLoading}>
              {isLoading ? 'Creating...' : 'Create'}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
