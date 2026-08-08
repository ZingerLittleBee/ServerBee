/**
 * Geo helpers for the server-map widget.
 *
 * Servers report an ISO 3166-1 alpha-2 `country_code` (MaxMind GeoIP), while
 * the choropleth's world topology (`public/world-countries.json`, Natural
 * Earth via topojson) keys features by alpha-3 `id`. This module bridges the
 * two and buckets server counts into the sequential `--chart-scale-*` fills.
 */

/** ISO 3166-1 alpha-2 → alpha-3. Includes `UK`→`GBR` alias and `XK`→`KOS`. */
export const ALPHA2_TO_ALPHA3: Record<string, string> = {
  AD: 'AND',
  AE: 'ARE',
  AF: 'AFG',
  AG: 'ATG',
  AI: 'AIA',
  AL: 'ALB',
  AM: 'ARM',
  AO: 'AGO',
  AQ: 'ATA',
  AR: 'ARG',
  AS: 'ASM',
  AT: 'AUT',
  AU: 'AUS',
  AW: 'ABW',
  AX: 'ALA',
  AZ: 'AZE',
  BA: 'BIH',
  BB: 'BRB',
  BD: 'BGD',
  BE: 'BEL',
  BF: 'BFA',
  BG: 'BGR',
  BH: 'BHR',
  BI: 'BDI',
  BJ: 'BEN',
  BL: 'BLM',
  BM: 'BMU',
  BN: 'BRN',
  BO: 'BOL',
  BQ: 'BES',
  BR: 'BRA',
  BS: 'BHS',
  BT: 'BTN',
  BV: 'BVT',
  BW: 'BWA',
  BY: 'BLR',
  BZ: 'BLZ',
  CA: 'CAN',
  CC: 'CCK',
  CD: 'COD',
  CF: 'CAF',
  CG: 'COG',
  CH: 'CHE',
  CI: 'CIV',
  CK: 'COK',
  CL: 'CHL',
  CM: 'CMR',
  CN: 'CHN',
  CO: 'COL',
  CR: 'CRI',
  CU: 'CUB',
  CV: 'CPV',
  CW: 'CUW',
  CX: 'CXR',
  CY: 'CYP',
  CZ: 'CZE',
  DE: 'DEU',
  DJ: 'DJI',
  DK: 'DNK',
  DM: 'DMA',
  DO: 'DOM',
  DZ: 'DZA',
  EC: 'ECU',
  EE: 'EST',
  EG: 'EGY',
  EH: 'ESH',
  ER: 'ERI',
  ES: 'ESP',
  ET: 'ETH',
  FI: 'FIN',
  FJ: 'FJI',
  FK: 'FLK',
  FM: 'FSM',
  FO: 'FRO',
  FR: 'FRA',
  GA: 'GAB',
  GB: 'GBR',
  GD: 'GRD',
  GE: 'GEO',
  GF: 'GUF',
  GG: 'GGY',
  GH: 'GHA',
  GI: 'GIB',
  GL: 'GRL',
  GM: 'GMB',
  GN: 'GIN',
  GP: 'GLP',
  GQ: 'GNQ',
  GR: 'GRC',
  GS: 'SGS',
  GT: 'GTM',
  GU: 'GUM',
  GW: 'GNB',
  GY: 'GUY',
  HK: 'HKG',
  HM: 'HMD',
  HN: 'HND',
  HR: 'HRV',
  HT: 'HTI',
  HU: 'HUN',
  ID: 'IDN',
  IE: 'IRL',
  IL: 'ISR',
  IM: 'IMN',
  IN: 'IND',
  IO: 'IOT',
  IQ: 'IRQ',
  IR: 'IRN',
  IS: 'ISL',
  IT: 'ITA',
  JE: 'JEY',
  JM: 'JAM',
  JO: 'JOR',
  JP: 'JPN',
  KE: 'KEN',
  KG: 'KGZ',
  KH: 'KHM',
  KI: 'KIR',
  KM: 'COM',
  KN: 'KNA',
  KP: 'PRK',
  KR: 'KOR',
  KW: 'KWT',
  KY: 'CYM',
  KZ: 'KAZ',
  LA: 'LAO',
  LB: 'LBN',
  LC: 'LCA',
  LI: 'LIE',
  LK: 'LKA',
  LR: 'LBR',
  LS: 'LSO',
  LT: 'LTU',
  LU: 'LUX',
  LV: 'LVA',
  LY: 'LBY',
  MA: 'MAR',
  MC: 'MCO',
  MD: 'MDA',
  ME: 'MNE',
  MF: 'MAF',
  MG: 'MDG',
  MH: 'MHL',
  MK: 'MKD',
  ML: 'MLI',
  MM: 'MMR',
  MN: 'MNG',
  MO: 'MAC',
  MP: 'MNP',
  MQ: 'MTQ',
  MR: 'MRT',
  MS: 'MSR',
  MT: 'MLT',
  MU: 'MUS',
  MV: 'MDV',
  MW: 'MWI',
  MX: 'MEX',
  MY: 'MYS',
  MZ: 'MOZ',
  NA: 'NAM',
  NC: 'NCL',
  NE: 'NER',
  NF: 'NFK',
  NG: 'NGA',
  NI: 'NIC',
  NL: 'NLD',
  NO: 'NOR',
  NP: 'NPL',
  NR: 'NRU',
  NU: 'NIU',
  NZ: 'NZL',
  OM: 'OMN',
  PA: 'PAN',
  PE: 'PER',
  PF: 'PYF',
  PG: 'PNG',
  PH: 'PHL',
  PK: 'PAK',
  PL: 'POL',
  PM: 'SPM',
  PN: 'PCN',
  PR: 'PRI',
  PS: 'PSE',
  PT: 'PRT',
  PW: 'PLW',
  PY: 'PRY',
  QA: 'QAT',
  RE: 'REU',
  RO: 'ROU',
  RS: 'SRB',
  RU: 'RUS',
  RW: 'RWA',
  SA: 'SAU',
  SB: 'SLB',
  SC: 'SYC',
  SD: 'SDN',
  SE: 'SWE',
  SG: 'SGP',
  SH: 'SHN',
  SI: 'SVN',
  SJ: 'SJM',
  SK: 'SVK',
  SL: 'SLE',
  SM: 'SMR',
  SN: 'SEN',
  SO: 'SOM',
  SR: 'SUR',
  SS: 'SSD',
  ST: 'STP',
  SV: 'SLV',
  SX: 'SXM',
  SY: 'SYR',
  SZ: 'SWZ',
  TC: 'TCA',
  TD: 'TCD',
  TF: 'ATF',
  TG: 'TGO',
  TH: 'THA',
  TJ: 'TJK',
  TK: 'TKL',
  TL: 'TLS',
  TM: 'TKM',
  TN: 'TUN',
  TO: 'TON',
  TR: 'TUR',
  TT: 'TTO',
  TV: 'TUV',
  TW: 'TWN',
  TZ: 'TZA',
  UA: 'UKR',
  UG: 'UGA',
  UK: 'GBR',
  UM: 'UMI',
  US: 'USA',
  UY: 'URY',
  UZ: 'UZB',
  VA: 'VAT',
  VC: 'VCT',
  VE: 'VEN',
  VG: 'VGB',
  VI: 'VIR',
  VN: 'VNM',
  VU: 'VUT',
  WF: 'WLF',
  WS: 'WSM',
  XK: 'KOS',
  YE: 'YEM',
  YT: 'MYT',
  ZA: 'ZAF',
  ZM: 'ZMB',
  ZW: 'ZWE'
}

const ALPHA3_TO_ALPHA2: Record<string, string> = Object.fromEntries(
  Object.entries(ALPHA2_TO_ALPHA3).map(([alpha2, alpha3]) => [alpha3, alpha2])
)

export function alpha2ToAlpha3(code: string | null | undefined): string | undefined {
  if (!code) {
    return undefined
  }
  return ALPHA2_TO_ALPHA3[code.toUpperCase()]
}

/** Reverse lookup for topojson feature ids (alpha-3 → alpha-2). */
export function alpha3ToAlpha2(code: string | null | undefined): string | undefined {
  if (!code) {
    return undefined
  }
  return ALPHA3_TO_ALPHA2[code.toUpperCase()]
}

export interface CountryServerGroup {
  /** ISO alpha-2 code, for localized names via `countryCodeToName`. */
  alpha2: string
  count: number
  serverNames: string[]
}

export interface GeoServerLike {
  country_code: string | null
  name: string
}

/**
 * Group servers by country, keyed by alpha-3 (the choropleth feature id).
 * Servers without a (mappable) country code are skipped.
 */
export function buildCountryServerGroups(servers: GeoServerLike[]): Map<string, CountryServerGroup> {
  const groups = new Map<string, CountryServerGroup>()

  for (const server of servers) {
    const alpha2 = server.country_code?.toUpperCase()
    const alpha3 = alpha2ToAlpha3(alpha2)
    if (!(alpha2 && alpha3)) {
      continue
    }
    const existing = groups.get(alpha3)
    if (existing) {
      existing.count += 1
      existing.serverNames.push(server.name)
    } else {
      groups.set(alpha3, { alpha2, count: 1, serverNames: [server.name] })
    }
  }

  return groups
}

/** Fill for a country with `count` servers; `undefined` count means no servers. */
export function countryServerFill(count: number | undefined, maxCount: number): string {
  if (!count || maxCount <= 0) {
    return 'var(--muted)'
  }
  const ratio = count / maxCount
  if (ratio > 0.75) {
    return 'var(--chart-scale-05)'
  }
  if (ratio > 0.5) {
    return 'var(--chart-scale-04)'
  }
  if (ratio > 0.25) {
    return 'var(--chart-scale-03)'
  }
  return 'var(--chart-scale-02)'
}
