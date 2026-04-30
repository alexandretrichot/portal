import { createFileRoute, Outlet, Navigate } from '@tanstack/react-router'
import { useAuth } from '@clerk/react'

export const Route = createFileRoute('/_auth')({
  component: AuthLayout,
})

function AuthLayout() {
  const { isSignedIn, isLoaded } = useAuth()

  if (!isLoaded) {
    return (
      <div className="flex h-screen items-center justify-center">
        <div className="animate-spin h-8 w-8 border-4 border-primary border-t-transparent rounded-full" />
      </div>
    )
  }

  if (!isSignedIn) {
    return <Navigate to="/sign-in" />
  }

  return <Outlet />
}
