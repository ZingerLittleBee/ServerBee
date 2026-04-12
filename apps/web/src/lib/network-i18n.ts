interface TranslateOptions extends Record<string, unknown> {
  defaultValue?: string
}

type Translate = (key: string, options?: TranslateOptions) => string

interface NetworkTargetNameParts {
  location: string
  name: string
  provider: string
  source: string | null
}

const LOCATION_I18N_KEYS: Record<string, string> = {
  Anhui: 'location_anhui',
  Beijing: 'location_beijing',
  Chongqing: 'location_chongqing',
  Fujian: 'location_fujian',
  Gansu: 'location_gansu',
  Guangdong: 'location_guangdong',
  Guangxi: 'location_guangxi',
  Guizhou: 'location_guizhou',
  Hainan: 'location_hainan',
  Hebei: 'location_hebei',
  Heilongjiang: 'location_heilongjiang',
  Henan: 'location_henan',
  Hubei: 'location_hubei',
  Hunan: 'location_hunan',
  InnerMongolia: 'location_inner_mongolia',
  Jiangsu: 'location_jiangsu',
  Jiangxi: 'location_jiangxi',
  Jilin: 'location_jilin',
  Liaoning: 'location_liaoning',
  Ningxia: 'location_ningxia',
  Qinghai: 'location_qinghai',
  Shaanxi: 'location_shaanxi',
  Shandong: 'location_shandong',
  Shanghai: 'location_shanghai',
  Shanxi: 'location_shanxi',
  Sichuan: 'location_sichuan',
  Tianjin: 'location_tianjin',
  Tibet: 'location_tibet',
  Tokyo: 'location_tokyo',
  US: 'location_us',
  Xinjiang: 'location_xinjiang',
  Yunnan: 'location_yunnan',
  Zhejiang: 'location_zhejiang'
}

const PROVIDER_SHORT_I18N_KEYS: Record<string, string> = {
  Mobile: 'provider_short_mobile',
  Telecom: 'provider_short_telecom',
  Unicom: 'provider_short_unicom'
}

function getNetworkLocationLabel(t: Translate, location: string): string {
  const key = LOCATION_I18N_KEYS[location]
  return key ? t(key, { defaultValue: location }) : location
}

function getNetworkProviderShortLabel(t: Translate, provider: string): string {
  const key = PROVIDER_SHORT_I18N_KEYS[provider]
  return key ? t(key, { defaultValue: provider }) : provider
}

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

export function getNetworkTargetDisplayName(
  t: Translate,
  language: string | undefined,
  target: NetworkTargetNameParts
): string {
  if (!(language?.startsWith('zh') && target.source)) {
    return target.name
  }

  const locationLabel = getNetworkLocationLabel(t, target.location)
  const providerLabel = getNetworkProviderShortLabel(t, target.provider)

  if (target.provider in PROVIDER_SHORT_I18N_KEYS) {
    return `${locationLabel}${providerLabel}`
  }

  const localizedName = target.name.replace(target.location, locationLabel).replace(target.provider, providerLabel)

  if (localizedName !== target.name) {
    return localizedName
  }

  if (locationLabel !== target.location) {
    return `${target.name} (${locationLabel})`
  }

  return target.name
}
