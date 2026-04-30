import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { api } from './client'
import type { DashboardData, ApiResponse, McpConfig } from './types'

export function useDashboard() {
  return useQuery({
    queryKey: ['dashboard'],
    queryFn: () => api.get<DashboardData>('/api/dashboard'),
  })
}

export function useCreateDevice() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (data: { name: string; alias: string }) =>
      api.post<ApiResponse<{ id: string }>>('/api/devices', data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['dashboard'] })
    },
  })
}

export function useDeleteDevice() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (deviceId: string) =>
      api.delete<ApiResponse<void>>(`/api/devices/${deviceId}`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['dashboard'] })
    },
  })
}

export function useRegenerateDeviceKey() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (deviceId: string) =>
      api.post<ApiResponse<{ key: string }>>(`/api/devices/${deviceId}/regenerate`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['dashboard'] })
    },
  })
}

export function useRegenerateGatewayKey() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: () => api.post<ApiResponse<{ key: string }>>('/api/gateway/regenerate'),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['dashboard'] })
    },
  })
}

export function useDeviceMcpConfig(deviceId: string) {
  return useQuery({
    queryKey: ['device', deviceId, 'mcp-config'],
    queryFn: () => api.get<ApiResponse<McpConfig>>(`/api/devices/${deviceId}/mcp-config`),
    enabled: !!deviceId,
  })
}

export function useUpdateMcpConfig() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ deviceId, config }: { deviceId: string; config: McpConfig }) =>
      api.post<ApiResponse<void>>(`/api/devices/${deviceId}/mcp-config`, config),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['dashboard'] })
    },
  })
}

export function useRestartDevice() {
  return useMutation({
    mutationFn: (deviceKey: string) =>
      api.post<ApiResponse<void>>(`/api/devices/${deviceKey}/restart`),
  })
}

export function useOpenSettings() {
  return useMutation({
    mutationFn: ({ deviceKey, permission }: { deviceKey: string; permission: string }) =>
      api.post<ApiResponse<void>>(`/api/devices/${deviceKey}/open-settings`, { permission }),
  })
}
