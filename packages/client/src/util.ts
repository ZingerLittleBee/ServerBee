export function convertFormatDataToString(obj: any) {
    const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB', 'PiB', 'EiB', '°C']

    if (
        Array.isArray(obj) &&
        obj.length === 2 &&
        !isNaN(obj[0]) &&
        units.includes(obj[1])
    ) {
        return obj.join(' ')
    }

    if (typeof obj === 'object' && obj !== null) {
        for (const key in obj) {
            obj[key] = convertFormatDataToString(obj[key])
        }
    }
    return obj
}
