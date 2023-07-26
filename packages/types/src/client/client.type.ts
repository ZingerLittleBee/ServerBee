import { Os, Overview, Process, Realtime, SimpleProcess } from "../fusion.type";
import { TokenPayload } from "../token/index";
import { DiskDetailDto, NetworkInfoDto } from "./device.type";

type CreateClientDto = {
    name: string
    userId?: string
}

type UpdateClientDto = {
    name?: string
    device?: DeviceDto
}

export type CreateDeviceDto = DeviceDto

type DeviceDto = {
    name?: string
    hostname?: string
    kernel?: string
    cpu_num?: number
    brand?: string
    frequency?: string
    vendor?: string
    memory?: string
    swap?: string
    version?: string
    disk?: DiskDetailDto[]
    network?: NetworkInfoDto[]
}


type FusionDto = {
    overview?: Overview
    os?: Os
    realtime?: Realtime
    simpleProcess?: SimpleProcess[]
    fullProcess?: Process[]
    clientId?: string
    time?: number
}

type ClientPayload = TokenPayload

type UpdateDeviceDto = {
    clientId: string
    device: DeviceDto
}

export type RegisterClientDto = {
    clientId: string
    userId: string
    device: DeviceDto
}


export {
    ClientPayload, CreateClientDto, DeviceDto,
    FusionDto,
    UpdateDeviceDto
};

