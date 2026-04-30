import { useRouter } from '@tanstack/react-router'
import { AlertCircle, RefreshCw } from 'lucide-react'
import { Button } from '@/components/ui/button'

interface ErrorComponentProps {
  error: Error
  reset?: () => void
}

export function ErrorComponent({ error, reset }: ErrorComponentProps) {
  const router = useRouter()

  return (
    <div className="flex min-h-[400px] flex-col items-center justify-center gap-4 p-8">
      <AlertCircle className="h-12 w-12 text-destructive" />
      <div className="text-center">
        <h2 className="text-lg font-semibold">Something went wrong</h2>
        <p className="text-sm text-muted-foreground mt-1">
          {error.message || 'An unexpected error occurred'}
        </p>
      </div>
      <div className="flex gap-2">
        <Button
          variant="outline"
          onClick={() => reset?.() || router.invalidate()}
        >
          <RefreshCw className="h-4 w-4 mr-2" />
          Try again
        </Button>
      </div>
    </div>
  )
}

export function NotFoundComponent() {
  const router = useRouter()

  return (
    <div className="flex min-h-[400px] flex-col items-center justify-center gap-4 p-8">
      <h2 className="text-2xl font-bold">404</h2>
      <p className="text-muted-foreground">Page not found</p>
      <Button onClick={() => router.navigate({ to: '/' })}>
        Go home
      </Button>
    </div>
  )
}
