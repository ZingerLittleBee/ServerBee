interface TranslateOptions {
  defaultValue?: string
}

type Translate = (key: string, options?: TranslateOptions) => string

export function getNetworkProbeTypeLabel(t: Translate, probeType: string): string {
  switch (probeType) {
    case 'icmp':
      return t('probe_type_icmp', { defaultValue: 'ICMP (Ping)' })
    case 'tcp':
      return t('probe_type_tcp', { defaultValue: 'TCP' })
    case 'http':
      return t('probe_type_http', { defaultValue: 'HTTP' })
    default:
      return probeType
  }
}
