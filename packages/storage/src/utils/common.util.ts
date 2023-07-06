import { FormatData } from '@server-octopus/types'

export const formatDataToString = (data?: FormatData): string => {
    if (!data) return ''
    const [value, unit] = data
    return value === '0' || value === '0.0' ? value : `${value} ${unit}`
}
