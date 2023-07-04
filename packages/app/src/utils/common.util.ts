import { FormatData } from '@/types/fusion.type'

export const formatDataToString = (data?: FormatData): string => {
    if (!data) return ''
    const [value, unit] = data
    return value === '0' || value === '0.0' ? value : `${value} ${unit}`
}
