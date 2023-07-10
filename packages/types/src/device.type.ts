import { FormatData } from './fusion.type'

export type DiskDetail = {
    disk_type: string
    device_name: string
    file_system: string
    total_space: FormatData
    available_space: FormatData
    is_removable: boolean
}

export type DiskDetailDto = Omit<
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

export type NetworkInfoDto = Omit<NetworkInfo, 'rx' | 'tx'> & {
    rx: string
    tx: string
}

type CreateDevice = {
    name: string
    hostname: string
    kernel: string
    cpu_num: number
    brand: string
    frequency: string
    vendor: string
    memory: FormatData
    swap: FormatData
    version: string
    disk: DiskDetail[]
    network: NetworkInfo[]
}

export {
    CreateDevice
}
