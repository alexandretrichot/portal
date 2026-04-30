import { createFileRoute } from '@tanstack/react-router'
import { UserButton, useUser } from '@clerk/react'
import { useSuspenseQuery } from '@tanstack/react-query'
import { toast } from 'sonner'
import {
  useCreateDevice,
  useDeleteDevice,
  useRegenerateDeviceKey,
  useRegenerateGatewayKey,
  useRestartDevice,
  useOpenSettings,
} from '@/api/hooks'
import { api } from '@/api/client'
import type { DashboardData } from '@/api/types'
import { GatewayCard } from '@/components/dashboard/gateway-card'
import { DeviceCard } from '@/components/dashboard/device-card'
import { CreateDeviceDialog } from '@/components/dashboard/create-device-dialog'
import { Skeleton } from '@/components/ui/skeleton'
import { ErrorComponent } from '@/components/error-boundary'
import { Suspense } from 'react'

export const Route = createFileRoute('/_auth/dashboard')({
  component: DashboardPage,
  errorComponent: ErrorComponent,
})

function DashboardPage() {
  return (
    <div className="min-h-screen bg-background">
      <DashboardHeader />
      <main className="mx-auto max-w-2xl px-4 py-8 space-y-6">
        <Suspense fallback={<DashboardSkeleton />}>
          <DashboardContent />
        </Suspense>
      </main>
    </div>
  )
}

function DashboardHeader() {
  const { user } = useUser()

  return (
    <header className="sticky top-0 z-50 border-b bg-background/95 backdrop-blur">
      <div className="mx-auto max-w-2xl px-4 flex h-14 items-center justify-between">
        <h1 className="text-lg font-semibold">Portal</h1>
        <div className="flex items-center gap-3">
          <span className="text-sm text-muted-foreground">
            {user?.primaryEmailAddress?.emailAddress}
          </span>
          <UserButton />
        </div>
      </div>
    </header>
  )
}

function DashboardContent() {
  const { data } = useSuspenseQuery({
    queryKey: ['dashboard'],
    queryFn: () => api.get<DashboardData>('/api/dashboard'),
    refetchInterval: 3000,
  })

  const createDevice = useCreateDevice()
  const deleteDevice = useDeleteDevice()
  const regenerateDeviceKey = useRegenerateDeviceKey()
  const regenerateGatewayKey = useRegenerateGatewayKey()
  const restartDevice = useRestartDevice()
  const openSettings = useOpenSettings()

  const handleCreateDevice = (formData: { name: string; alias: string }) => {
    createDevice.mutate(formData, {
      onSuccess: () => toast.success('Device created'),
      onError: () => toast.error('Failed to create device'),
    })
  }

  const handleDeleteDevice = (id: string) => {
    if (!confirm('Delete this device?')) return
    deleteDevice.mutate(id, {
      onSuccess: () => toast.success('Device deleted'),
      onError: () => toast.error('Failed to delete device'),
    })
  }

  const handleRegenerateDeviceKey = (id: string) => {
    if (!confirm('Regenerate key? The device will need to reconnect.')) return
    regenerateDeviceKey.mutate(id, {
      onSuccess: () => toast.success('Key regenerated'),
      onError: () => toast.error('Failed to regenerate key'),
    })
  }

  const handleRegenerateGatewayKey = () => {
    if (!confirm('Regenerate gateway key? All existing integrations will stop working.')) return
    regenerateGatewayKey.mutate(undefined, {
      onSuccess: () => toast.success('Gateway key regenerated'),
      onError: () => toast.error('Failed to regenerate gateway key'),
    })
  }

  const handleRestartDevice = (key: string) => {
    restartDevice.mutate(key, {
      onSuccess: () => toast.success('Device restarting...'),
      onError: () => toast.error('Failed to restart device'),
    })
  }

  const handleOpenSettings = (key: string, permission: string) => {
    openSettings.mutate({ deviceKey: key, permission })
  }

  return (
    <>
      <GatewayCard
        mcpUrl={data.gateway.mcpUrl}
        onRegenerate={handleRegenerateGatewayKey}
        isRegenerating={regenerateGatewayKey.isPending}
      />

      <section>
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-lg font-semibold">Devices</h2>
          <CreateDeviceDialog
            onSubmit={handleCreateDevice}
            isLoading={createDevice.isPending}
          />
        </div>

        {data.devices.length === 0 ? (
          <div className="text-center py-12 border rounded-lg bg-muted/30">
            <p className="text-muted-foreground">
              No devices yet. Create one to connect your machines.
            </p>
          </div>
        ) : (
          <div className="space-y-4">
            {data.devices.map((device) => (
              <DeviceCard
                key={device.id}
                device={device}
                onDelete={handleDeleteDevice}
                onRegenerateKey={handleRegenerateDeviceKey}
                onRestart={handleRestartDevice}
                onOpenSettings={handleOpenSettings}
              />
            ))}
          </div>
        )}
      </section>
    </>
  )
}

function DashboardSkeleton() {
  return (
    <div className="space-y-6">
      <Skeleton className="h-36 w-full rounded-lg" />
      <div className="space-y-4">
        <div className="flex items-center justify-between">
          <Skeleton className="h-5 w-20" />
          <Skeleton className="h-9 w-28" />
        </div>
        <Skeleton className="h-48 w-full rounded-lg" />
      </div>
    </div>
  )
}
