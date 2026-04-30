import { createRootRouteWithContext, Outlet } from '@tanstack/react-router'
import type { QueryClient } from '@tanstack/react-query'
import { Toaster } from '@/components/ui/sonner'
import { ErrorComponent, NotFoundComponent } from '@/components/error-boundary'

interface RouterContext {
  queryClient: QueryClient
}

export const Route = createRootRouteWithContext<RouterContext>()({
  component: RootLayout,
  errorComponent: ErrorComponent,
  notFoundComponent: NotFoundComponent,
})

function RootLayout() {
  return (
    <>
      <Outlet />
      <Toaster />
    </>
  )
}
