import { createFileRoute } from '@tanstack/react-router'
import { UserButton, useUser } from '@clerk/react'

export const Route = createFileRoute('/_auth/dashboard')({
  component: Dashboard,
})

function Dashboard() {
  const { user } = useUser()

  return (
    <div className="container mx-auto py-8 px-4">
      <header className="flex items-center justify-between mb-8">
        <h1 className="text-2xl font-bold">Dashboard</h1>
        <div className="flex items-center gap-4">
          <span className="text-muted-foreground">
            {user?.primaryEmailAddress?.emailAddress}
          </span>
          <UserButton afterSignOutUrl="/sign-in" />
        </div>
      </header>

      <main>
        <div className="rounded-lg border bg-card p-6">
          <h2 className="text-lg font-semibold mb-4">Welcome back!</h2>
          <p className="text-muted-foreground">
            Authentication is working. Next step: integrate the device management UI.
          </p>
        </div>
      </main>
    </div>
  )
}
