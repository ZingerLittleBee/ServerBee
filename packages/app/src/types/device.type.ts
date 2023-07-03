import { FormatData } from '@/types/fusion.type'

export type DiskDetail = {
    disk_type: string
    device_name: string
    file_system: string
    total_space: FormatData
    available_space: FormatData
    is_removable: boolean
}

export type NetworkInfo = {
    name: string
    // ip: string
    mac: string
    rx: FormatData
    tx: FormatData
}
