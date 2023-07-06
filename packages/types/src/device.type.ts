import {FormatData} from "./fusion.type";

export type DiskDetail = {
    disk_type: string
    device_name: string
    file_system: string
    total_space: FormatData
    available_space: FormatData
    is_removable: boolean
}

export type DiskDetailEntity = Omit<
    DiskDetail,
    'total_space' | 'available_space'
> & {
    total_space: string
    available_space: string
}

export type NetworkInfo = {
    name: string
    // ip: string
    mac: string
    rx: FormatData
    tx: FormatData
}

export type NetworkInfoEntity = Omit<NetworkInfo, 'rx' | 'tx'> & {
    rx: string
    tx: string
}
