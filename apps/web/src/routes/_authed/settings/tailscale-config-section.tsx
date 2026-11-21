import { useMutation, useQueryClient } from '@tanstack/react-query'
import { CheckCircle2, PlugZap, Save, XCircle } from 'lucide-react'
import { type FormEvent, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Separator } from '@/components/ui/separator'
import { Switch } from '@/components/ui/switch'
import { api } from '@/lib/api-client'
import type {
  TailscaleAdminConfigResponse,
  TailscaleConnectionTestResponse,
  TestTailscaleConfigRequest,
  UpdateTailscaleConfigRequest
} from '@/lib/api-schema'

const AUTO_PROVISION_DISABLED_VALUE = '__disabled__'

interface TailscaleConfigSectionProps {
  config: TailscaleAdminConfigResponse | undefined
  isLoading: boolean
}

export function TailscaleConfigSection({ config, isLoading }: TailscaleConfigSectionProps) {
  const { t } = useTranslation('tailscale')
  const queryClient = useQueryClient()

  const [enabled, setEnabled] = useState(false)
  const [provider, setProvider] = useState('tailscale')
  const [apiBaseUrl, setApiBaseUrl] = useState('https://api.tailscale.com')
  const [tailnet, setTailnet] = useState('-')
  const [apiKey, setApiKey] = useState('')
  const [preferredServerUrl, setPreferredServerUrl] = useState('')
  const [authEnabled, setAuthEnabled] = useState(false)
  const [autoProvisionRole, setAutoProvisionRole] = useState('')
  const [testResult, setTestResult] = useState<TailscaleConnectionTestResponse | null>(null)

  useEffect(() => {
    if (!config) {
      return
    }
    setEnabled(config.enabled)
    setProvider(config.provider || 'tailscale')
    setApiBaseUrl(config.api_base_url || 'https://api.tailscale.com')
    setTailnet(config.tailnet || '-')
    setPreferredServerUrl(config.preferred_server_url || '')
    setAuthEnabled(config.auth_enabled)
    setAutoProvisionRole(config.auto_provision_role || '')
    setApiKey('')
  }, [config])

  const saveMutation = useMutation({
    mutationFn: (payload: UpdateTailscaleConfigRequest) =>
      api.put<TailscaleAdminConfigResponse>('/api/tailscale/config', payload),
    onSuccess: (updated) => {
      setApiKey('')
      queryClient.setQueryData(['tailscale', 'config'], updated)
      queryClient.invalidateQueries({ queryKey: ['tailscale', 'overview'] }).catch(() => undefined)
      toast.success(t('settings.savedToast'))
    },
    onError: (err: unknown) => {
      toast.error(t('settings.saveFailedToast'), {
        description: err instanceof Error ? err.message : undefined
      })
    }
  })

  const testMutation = useMutation({
    mutationFn: (payload: TestTailscaleConfigRequest) =>
      api.post<TailscaleConnectionTestResponse>('/api/tailscale/test', payload),
    onSuccess: (result) => {
      setTestResult(result)
      if (result.success) {
        toast.success(t('settings.testSuccess', { count: result.device_count }))
      } else {
        toast.error(t('settings.testFailed'), { description: result.message })
      }
    },
    onError: (err: unknown) => {
      const message = err instanceof Error ? err.message : t('settings.testFailed')
      setTestResult({ success: false, device_count: 0, message })
      toast.error(t('settings.testFailed'), { description: message })
    }
  })

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault()
    const trimmedKey = apiKey.trim()
    saveMutation.mutate({
      enabled,
      provider,
      api_base_url: apiBaseUrl.trim() || 'https://api.tailscale.com',
      tailnet: tailnet.trim() || '-',
      api_key: trimmedKey ? trimmedKey : undefined,
      preferred_server_url: preferredServerUrl.trim(),
      auth_enabled: authEnabled,
      auto_provision_role: autoProvisionRole
    })
  }

  const handleTestConnection = () => {
    const trimmedKey = apiKey.trim()
    testMutation.mutate({
      provider,
      api_base_url: apiBaseUrl.trim() || undefined,
      tailnet: tailnet.trim() || undefined,
      api_key: trimmedKey || undefined
    })
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t('settings.adminCardTitle')}</CardTitle>
        <CardDescription>{t('settings.adminCardDescription')}</CardDescription>
      </CardHeader>
      <CardContent>
        <form className="space-y-6" onSubmit={handleSubmit}>
          <div className="flex items-center justify-between gap-4">
            <div className="space-y-0.5">
              <label className="font-medium text-sm" htmlFor="tailscale-enabled">
                {t('settings.enabledLabel')}
              </label>
              <p className="text-muted-foreground text-xs">{t('settings.enabledHint')}</p>
            </div>
            <Switch
              aria-label={t('settings.enabledLabel')}
              checked={enabled}
              disabled={isLoading}
              id="tailscale-enabled"
              onCheckedChange={(checked) => setEnabled(Boolean(checked))}
            />
          </div>

          <div className="grid gap-4 sm:grid-cols-2">
            <div className="space-y-1.5">
              <label className="font-medium text-sm" htmlFor="tailscale-provider">
                {t('settings.providerLabel')}
              </label>
              <Select
                items={{
                  tailscale: t('settings.providerTailscale'),
                  headscale: t('settings.providerHeadscale')
                }}
                onValueChange={(value) => {
                  if (!value) {
                    return
                  }
                  setProvider(value)
                  if (value === 'tailscale' && !apiBaseUrl.trim()) {
                    setApiBaseUrl('https://api.tailscale.com')
                  }
                }}
                value={provider}
              >
                <SelectTrigger aria-label={t('settings.providerLabel')} className="w-full" id="tailscale-provider">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="tailscale">{t('settings.providerTailscale')}</SelectItem>
                  <SelectItem value="headscale">{t('settings.providerHeadscale')}</SelectItem>
                </SelectContent>
              </Select>
            </div>

            <div className="space-y-1.5">
              <label className="font-medium text-sm" htmlFor="tailscale-tailnet">
                {t('settings.tailnetLabel')}
              </label>
              <Input
                autoComplete="off"
                disabled={isLoading}
                id="tailscale-tailnet"
                onChange={(e) => setTailnet(e.target.value)}
                placeholder={t('settings.tailnetPlaceholder')}
                value={tailnet}
              />
              <p className="text-muted-foreground text-xs">{t('settings.tailnetHint')}</p>
            </div>
          </div>

          <div className="grid gap-4 sm:grid-cols-2">
            <div className="space-y-1.5">
              <label className="font-medium text-sm" htmlFor="tailscale-api-base-url">
                {t('settings.apiBaseUrlLabel')}
              </label>
              <Input
                autoComplete="off"
                disabled={isLoading}
                id="tailscale-api-base-url"
                onChange={(e) => setApiBaseUrl(e.target.value)}
                placeholder={provider === 'headscale' ? 'https://headscale.example.com' : 'https://api.tailscale.com'}
                value={apiBaseUrl}
              />
              <p className="text-muted-foreground text-xs">
                {provider === 'headscale'
                  ? t('settings.apiBaseUrlHintHeadscale')
                  : t('settings.apiBaseUrlHintTailscale')}
              </p>
            </div>

            <div className="space-y-1.5">
              <div className="flex items-center justify-between gap-2">
                <label className="font-medium text-sm" htmlFor="tailscale-api-key">
                  {t('settings.apiKeyLabel')}
                </label>
                {config?.has_api_key && (
                  <Badge
                    className="border-emerald-500/30 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400"
                    variant="outline"
                  >
                    {t('settings.apiKeySavedBadge')}
                  </Badge>
                )}
              </div>
              <Input
                autoComplete="new-password"
                disabled={isLoading}
                id="tailscale-api-key"
                onChange={(e) => setApiKey(e.target.value)}
                placeholder={
                  config?.has_api_key ? t('settings.apiKeyPlaceholderSet') : t('settings.apiKeyPlaceholderEmpty')
                }
                type="password"
                value={apiKey}
              />
              <p className="text-muted-foreground text-xs">{t('settings.apiKeyHint')}</p>
            </div>
          </div>

          <div className="space-y-1.5">
            <label className="font-medium text-sm" htmlFor="tailscale-preferred-server-url">
              {t('settings.preferredServerUrlLabel')}
            </label>
            <Input
              autoComplete="off"
              disabled={isLoading}
              id="tailscale-preferred-server-url"
              onChange={(e) => setPreferredServerUrl(e.target.value)}
              placeholder={t('settings.preferredServerUrlPlaceholder')}
              value={preferredServerUrl}
            />
            <p className="text-muted-foreground text-xs">{t('settings.preferredServerUrlHint')}</p>
          </div>

          <Separator />

          <div className="space-y-4">
            <h3 className="font-semibold text-sm">{t('settings.authSectionTitle')}</h3>

            <div className="flex items-center justify-between gap-4">
              <div className="space-y-0.5">
                <label className="font-medium text-sm" htmlFor="tailscale-auth-enabled">
                  {t('settings.authEnabledLabel')}
                </label>
                <p className="text-muted-foreground text-xs">{t('settings.authEnabledHint')}</p>
              </div>
              <Switch
                aria-label={t('settings.authEnabledLabel')}
                checked={authEnabled}
                disabled={isLoading}
                id="tailscale-auth-enabled"
                onCheckedChange={(checked) => setAuthEnabled(Boolean(checked))}
              />
            </div>

            <div className="max-w-sm space-y-1.5">
              <label className="font-medium text-sm" htmlFor="tailscale-auto-provision-role">
                {t('settings.autoProvisionRoleLabel')}
              </label>
              <Select
                items={{
                  [AUTO_PROVISION_DISABLED_VALUE]: t('settings.autoProvisionDisabled'),
                  member: t('settings.autoProvisionMember')
                }}
                onValueChange={(value) => {
                  setAutoProvisionRole(!value || value === AUTO_PROVISION_DISABLED_VALUE ? '' : value)
                }}
                value={autoProvisionRole || AUTO_PROVISION_DISABLED_VALUE}
              >
                <SelectTrigger
                  aria-label={t('settings.autoProvisionRoleLabel')}
                  className="w-full"
                  id="tailscale-auto-provision-role"
                >
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={AUTO_PROVISION_DISABLED_VALUE}>{t('settings.autoProvisionDisabled')}</SelectItem>
                  <SelectItem value="member">{t('settings.autoProvisionMember')}</SelectItem>
                </SelectContent>
              </Select>
              <p className="text-muted-foreground text-xs">{t('settings.autoProvisionHint')}</p>
            </div>
          </div>

          {testResult && (
            <output
              className={`flex items-center gap-2 rounded-md border px-3 py-2 text-xs ${
                testResult.success
                  ? 'border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300'
                  : 'border-destructive/30 bg-destructive/10 text-destructive'
              }`}
            >
              {testResult.success ? (
                <CheckCircle2 aria-hidden="true" className="size-4 shrink-0" />
              ) : (
                <XCircle aria-hidden="true" className="size-4 shrink-0" />
              )}
              <span>
                {testResult.success
                  ? `${t('settings.testSuccess', { count: testResult.device_count })} — ${testResult.message}`
                  : testResult.message}
              </span>
            </output>
          )}

          <div className="flex flex-wrap items-center justify-end gap-2">
            <Button
              disabled={isLoading || testMutation.isPending}
              onClick={handleTestConnection}
              size="sm"
              type="button"
              variant="outline"
            >
              <PlugZap aria-hidden="true" className="mr-1.5 size-4" />
              {testMutation.isPending ? t('settings.testingConnection') : t('settings.testConnection')}
            </Button>
            <Button disabled={isLoading || saveMutation.isPending} size="sm" type="submit">
              <Save aria-hidden="true" className="mr-1.5 size-4" />
              {saveMutation.isPending ? t('settings.saving') : t('settings.save')}
            </Button>
          </div>
        </form>
      </CardContent>
    </Card>
  )
}
