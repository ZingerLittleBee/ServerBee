import { DiskDetail } from "./client/device.type";

export type FormatData = [string, 'B' | 'KiB' | 'MiB' | 'GiB' | 'TiB' | 'PiB' | 'EiB' | '°C'];

export type DiskIO = {
    read: FormatData
    total_read: FormatData
    write: FormatData
    total_write: FormatData
}

export type MemUsage = {
    total: FormatData
    used: FormatData
    free: FormatData
    swap_total: FormatData
    swap_used: FormatData
    swap_free: FormatData
}

export type DiskUsage = {
    total: FormatData
    used: FormatData
    free: FormatData
}

export type NetworkIO = {
    rx: FormatData
    ttl_rx: FormatData
    tx: FormatData
    ttl_tx: FormatData
}

export interface Overview {
    load_avg: number[]
    cpu_usage: string
    memory_usage: MemUsage
    disk_usage: DiskUsage
    disk_io: DiskIO
    network_io: NetworkIO
}

export interface User {
    uid: string
    gid: string
    name: string
    groups: string[]
}

export type CupInfo = {
    core_num: number
    brand: string
    frequency: number
    vendor_id: string
}

export interface Os {
    name: string
    kernel_version: string
    os_version: string
    hostname: string
    cpu_info: CupInfo
    users: User[]
    boot_time: number
}

export type NetworkDetail = {
    name: string
    packet: FormatData[]
}

export type Realtime = {
    cpu: number[]
    network: NetworkDetail[]
    disk: DiskDetail[]
    uptime: number[]
    temp: Temp[]
}

export interface Temp {
    label: string
    temp: FormatData
}

export type Process = {
    name: string
    cmd: string[]
    exe: string
    pid: string
    environ: string[]
    cwd: string
    root: string
    memory: FormatData
    vir_mem: FormatData
    parent?: string
    status: string
    start_time: string
    run_time: string
    cpu: string
    disk: DiskIO
    user?: string
    children?: string[]
    // group_id?: string;
}

export type SimpleProcess = Pick<Process, 'name' | 'pid' | 'cpu' | 'memory'>

export type Fusion = {
    overview?: Overview
    os?: Os
    realtime?: Realtime
    process?: Process[]
    full_process?: Process[]
}


