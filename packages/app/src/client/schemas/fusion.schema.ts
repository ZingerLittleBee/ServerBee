import { Prop, Schema, SchemaFactory } from '@nestjs/mongoose'
import { Document } from 'mongoose'

export interface Overview {
    load_avg: number[]
    cpu_usage: string
    memory_usage: {
        total: string[]
        used: string[]
        free: string[]
        swap_total: string[]
        swap_used: string[]
        swap_free: string[]
    }
    disk_usage: {
        total: string[]
        used: string[]
        free: string[]
    }
    disk_io: {
        read: string[]
        total_read: string[]
        write: string[]
        total_write: string[]
    }
    network_io: {
        rx: string[]
        ttl_rx: string[]
        tx: string[]
        ttl_tx: string[]
    }
}

export interface User {
    uid: string
    gid: string
    name: string
    groups: string[]
}

export interface Os {
    name: string
    kernel_version: string
    os_version: string
    hostname: string
    cpu_info: {
        core_num: number
        brand: string
        frequency: number
        vendor_id: string
    }
    users: User[]
    boot_time: number
}

export interface Realtime {
    cpu: number[]
    network: {
        name: string
        packet: string[][]
    }[]
}

export interface Disk {
    disk_type: string
    device_name: string
    file_system: string
    total_space: string[]
    available_space: string[]
    is_removable: boolean
}

export interface Temp {
    label: string
    temp: string[]
}

export interface FusionDocument extends Document {
    overview: Overview
    os: Os
    realtime: Realtime
    disk: Disk[]
    uptime: number[]
    temp: Temp[]
}

@Schema()
export class Fusion {
    @Prop({ type: Object })
    overview: Overview

    @Prop({ type: Object })
    os: Os

    @Prop({ type: Object })
    realtime: Realtime

    @Prop({ type: [{ type: Object }] })
    disk: Disk[]

    @Prop({ type: [Number] })
    uptime: number[]

    @Prop({ type: [{ type: Object }] })
    temp: Temp[]

    @Prop({ type: String })
    clientId: string

    @Prop({ type: Number })
    time: number
}

export const FusionSchema = SchemaFactory.createForClass(Fusion)
